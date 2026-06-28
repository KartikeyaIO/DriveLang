use std::collections::{HashMap, HashSet};
use std::path::Path;
use fontdue::{Font, FontSettings};

use crate::media::track::Track;
use crate::media::video::TimeStamp;
use crate::text::Text;
use crate::filter::{Filter,AudioFilter,Effect, Instruction};
use crate::io::io::{self, IOError};
// use crate::io::video_io::{Video, VideoEncoder};
use crate::media::frame::{Color, Frame, Pos};
use crate::parser::{
    BinOp, Channel, ChannelAssign, Expr,EffectDecl,AudioFilterDecl, FilterDecl, Import, Item, Program, Statement,
};
use crate::pipeline::kernel::Kernel;
use crate::pipeline::pipeline::{AudioPipeline, EffectPipeline, Operation,AudioOperation, Pipeline, PipelineError};
use crate::range::{Mask, Rect, StepRange};
use std::cell::RefCell;
use std::rc::Rc;

// #[derive(Clone)]
// pub struct VideoHandle(pub Rc<RefCell<Video>>);


// impl std::fmt::Debug for VideoHandle {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         let vid = self.0.borrow();
//         write!(
//             f,
//             "Video {{ width: {}, height: {}, fps: {}, frames: {} }}",
//             vid.width(),
//             vid.height(),
//             vid.fps(),
//             vid.frame_count()
//         )
//     }
// }

#[derive(Debug, Clone)]
pub enum Value {
    //Video(VideoHandle),
    Frame(Frame),
    Track(Track),
    Number(f64),
    String(String),
}

#[derive(Debug)]
pub enum EngineError {
    Compile(String),
    Eval(String),
    Pipeline(PipelineError),
    Io(IOError),
    UndefinedVar(String),
    UndefinedOp(String),
    EvalError(String),
}

impl From<PipelineError> for EngineError {
    fn from(e: PipelineError) -> Self {
        EngineError::Pipeline(e)
    }
}
impl From<IOError> for EngineError {
    fn from(e: IOError) -> Self {
        EngineError::Io(e)
    }
}

fn compile_expr(
    expr: &Expr,
    params: &[String],
    param_count: usize,
    context: CompileContext,
) -> Result<Vec<Instruction>, EngineError> {
    let mut out = Vec::new();
    compile_into(expr, params, param_count, &mut out,&context)?;
    Ok(out)
}
fn compile_stmts_for_channel(
    stmts: &[Statement],
    target: &Channel,
    params: &[String],
    param_count: usize,
    local_scope: &mut Vec<String>,
    out: &mut Vec<Instruction>,
    context: &CompileContext,
) -> Result<(), EngineError> {
    for stmt in stmts {
        match stmt {
            Statement::Let { name, value } => {
                // Let bindings are always emitted — any channel may reference them.
                compile_into(value, local_scope, param_count, out, context)?;
                let local_index = local_scope.len() - param_count;
                out.push(Instruction::StoreLocal(local_index));
                local_scope.push(name.clone());
            }

            Statement::Channel(ChannelAssign { channel, value }) => {
                // Only emit instructions for the channel we're currently building.
                if channel == target {
                    compile_into(value, local_scope, param_count, out,context)?;
                }
            }

            Statement::IfElse { cond, true_branch, false_branch } => {
                // Emit the condition expression.
                compile_into(cond, local_scope, param_count, out,context)?;

                // Placeholder: jump over true branch if condition is false.
                let jif_idx = out.len();
                out.push(Instruction::JumpIfFalse(0));

                // True branch — branch-local let bindings must not leak out.
                let mut true_scope = local_scope.clone();
                compile_stmts_for_channel(
                    true_branch, target, params, param_count, &mut true_scope, out,context
                )?;

                // Placeholder: jump over false branch after true branch executes.
                let jump_idx = out.len();
                out.push(Instruction::Jump(0));

                // Patch the JumpIfFalse to land here (start of false branch).
                out[jif_idx] = Instruction::JumpIfFalse(out.len());

                // False branch.
                let mut false_scope = local_scope.clone();
                compile_stmts_for_channel(
                    false_branch, target, params, param_count, &mut false_scope, out,context
                )?;

                // Patch the Jump to land here (after false branch).
                out[jump_idx] = Instruction::Jump(out.len());
            }
        }
    }
    Ok(())
}

fn compile_channel_program(
    body: &[Statement],
    target: Channel,
    params: &[String],
    param_count: usize,
    context: &CompileContext
) -> Result<Vec<Instruction>, EngineError> {
    let mut out = Vec::new();
    // local_scope starts as a copy of params — same as the old code.
    let mut local_scope: Vec<String> = params.to_vec();
    compile_stmts_for_channel(body, &target, params, param_count, &mut local_scope, &mut out,context)?;

    // If the channel was never assigned, pass the original value through.
    if out.is_empty() {
        out.push(match target {
            Channel::R => Instruction::LoadR,
            Channel::G => Instruction::LoadG,
            Channel::B => Instruction::LoadB,
            Channel::A => Instruction::LoadA,
            Channel::T => Instruction::LoadT,
            Channel::L => Instruction::LoadL,
        });
    }
    Ok(out)
}

fn compile_into(
    expr: &Expr,
    params: &[String],
    param_count: usize,
    out: &mut Vec<Instruction>,
    context: &CompileContext,
) -> Result<(), EngineError> {
    match expr {
        Expr::Int(v) => out.push(Instruction::PushInt(*v)),
        Expr::Float(v) => out.push(Instruction::PushFloat(*v as f32)),

        Expr::Ident(name) => 
                match context {
                    CompileContext::Image => {
                        match name.as_str() {
                            "r" => out.push(Instruction::LoadR),
                            "g" => out.push(Instruction::LoadG),
                            "b" => out.push(Instruction::LoadB),
                            "a" => out.push(Instruction::LoadA),

                            "x" => out.push(Instruction::LoadX),
                            "y" => out.push(Instruction::LoadY),
                            "width" => out.push(Instruction::LoadWidth),
                            "height" => out.push(Instruction::LoadHeight),

                            other => {
                                if let Some(idx) = params.iter().position(|p| p == other) {
                                    if idx < param_count {
                                        out.push(Instruction::LoadParam(idx));
                                    } else {
                                        out.push(Instruction::LoadLocal(idx - param_count));
                                    }
                                } else {
                                    return Err(EngineError::Compile(format!(
                                        "unknown identifier '{other}' in image filter"
                                    )));
                                }
                            }
                        }
                    }

                    CompileContext::Audio => {
                        match name.as_str() {
                            "l" => out.push(Instruction::LoadL),
                            "r" => out.push(Instruction::LoadR),

                            "time" => out.push(Instruction::LoadTime),
                            "sr" => out.push(Instruction::LoadSampleRate),

                            other => {
                                if let Some(idx) = params.iter().position(|p| p == other) {
                                    if idx < param_count {
                                        out.push(Instruction::LoadParam(idx));
                                    } else {
                                        out.push(Instruction::LoadLocal(idx - param_count));
                                    }
                                } else {
                                    return Err(EngineError::Compile(format!(
                                        "unknown identifier '{other}' in audio filter"
                                    )));
                                }
                            }
                        }
                    }

                    CompileContext::Effect => {
                        match name.as_str() {
                            "r" => out.push(Instruction::LoadR),
                            "g" => out.push(Instruction::LoadG),
                            "b" => out.push(Instruction::LoadB),
                            "a" => out.push(Instruction::LoadA),

                            "t" => out.push(Instruction::LoadT),

                            "x" => out.push(Instruction::LoadX),
                            "y" => out.push(Instruction::LoadY),
                            "width" => out.push(Instruction::LoadWidth),
                            "height" => out.push(Instruction::LoadHeight),

                            other => {
                                if let Some(idx) = params.iter().position(|p| p == other) {
                                    if idx < param_count {
                                        out.push(Instruction::LoadParam(idx));
                                    } else {
                                        out.push(Instruction::LoadLocal(idx - param_count));
                                    }
                                } else {
                                    return Err(EngineError::Compile(format!(
                                        "unknown identifier '{other}' in effect"
                                    )));
                                }
                            }
                        }
                    }
                }
            

        Expr::Neg(inner) => {
            compile_into(inner, params, param_count, out,context)?;
            out.push(Instruction::Neg);
        }
        Expr::Not(inner) => {
            compile_into(inner, params, param_count, out,context)?;
            out.push(Instruction::Not);
        }

        Expr::BinOp { op, lhs, rhs } => {
            compile_into(lhs, params, param_count, out,context)?;
            compile_into(rhs, params, param_count, out,context)?;
            out.push(match op {
                BinOp::Add => Instruction::Add,
                BinOp::Sub => Instruction::Sub,
                BinOp::Mul => Instruction::Mul,
                BinOp::Div => Instruction::Div,
                // Add these:
                BinOp::Eq => Instruction::Eq,
                BinOp::Ne => Instruction::Ne,
                BinOp::Gt => Instruction::Gt,
                BinOp::Ge => Instruction::Ge,
                BinOp::Lt => Instruction::Lt,
                BinOp::Le => Instruction::Le,
                BinOp::And => Instruction::And,
                BinOp::Or => Instruction::Or,
            });
        }

        Expr::Call { path, args } => {
            let name = path
                .last()
                .ok_or_else(|| EngineError::Compile("empty call path".into()))?;

            // multi-arg builtins handled specially
            match (name.as_str(), args.len()) {
                ("clamp", 3) => {
                    compile_into(&args[0], params, param_count, out,context)?;
                    compile_into(&args[1], params, param_count, out,context)?;
                    compile_into(&args[2], params, param_count, out,context)?;
                    out.push(Instruction::Clamp);
                    return Ok(());
                }
                ("lerp", 3) => {
                    compile_into(&args[0], params, param_count, out,context)?;
                    compile_into(&args[1], params, param_count, out,context)?;
                    compile_into(&args[2], params, param_count, out,context)?;
                    out.push(Instruction::Lerp);
                    return Ok(());
                }
                ("smooth_lerp", 3) => {
                    compile_into(&args[0], params, param_count, out,context)?;
                    compile_into(&args[1], params, param_count, out,context)?;
                    compile_into(&args[2], params, param_count, out,context)?;
                    out.push(Instruction::SmoothLerp);
                    return Ok(());
                }
                
                ("min", 2) => {
                    compile_into(&args[0], params, param_count, out,context)?;
                    compile_into(&args[1], params, param_count, out,context)?;
                    out.push(Instruction::Min);
                    return Ok(());
                }
                ("max", 2) => {
                    compile_into(&args[0], params, param_count, out,context)?;
                    compile_into(&args[1], params, param_count, out,context)?;
                    out.push(Instruction::Max);
                    return Ok(());
                }
                ("pow", 2) => {
                    compile_into(&args[0], params, param_count, out,context)?;
                    compile_into(&args[1], params, param_count, out,context)?;
                    out.push(Instruction::Pow);
                    return Ok(());
                }
                _ => {}
            }

            // single-arg math functions
            if args.len() != 1 {
                return Err(EngineError::Compile(format!(
                    "unsupported call '{name}' with {} args",
                    args.len()
                )));
            }
            compile_into(&args[0], params, param_count, out,context)?;
            out.push(match name.as_str() {
                "abs" => Instruction::Abs,
                "sin" => Instruction::Sin,
                "cos" => Instruction::Cos,
                "tan" => Instruction::Tan,
                "asin" => Instruction::Asin,
                "acos" => Instruction::Acos,
                "atan" => Instruction::Atan,
                "sqrt" => Instruction::Sqrt,
                "exp" => Instruction::Exp,
                "log" => Instruction::Log,
                "log10" => Instruction::Log10,
                "floor" => Instruction::Floor,
                "ceil" => Instruction::Ceil,
                "round" => Instruction::Round,
                other => return Err(EngineError::Compile(format!("unknown function '{other}'"))),
            });
        }

        other => {
            return Err(EngineError::Compile(format!(
                "expression not valid in filter body: {other:?}"
            )));
        }
    }
    Ok(())
}
/// Compiler Context
enum CompileContext {
    Image,
    Audio,
    Effect,
    
}
pub fn compile_audiofilter_decl(decl: &AudioFilterDecl) -> Result<AudioFilter, EngineError> {
    let param_count = decl.params.len();
    Ok(AudioFilter {
        name: decl.name.clone(),
        params: decl.params.clone(),
        l_program: compile_channel_program(&decl.body, Channel::L, &decl.params, param_count, &CompileContext::Audio)?,
        r_program: compile_channel_program(&decl.body, Channel::R, &decl.params, param_count,&CompileContext::Audio)?,
    })
}

pub fn compile_filter_decl(decl: &FilterDecl) -> Result<Filter, EngineError> {
    let param_count = decl.params.len();
    Ok(Filter {
        name: decl.name.clone(),
        params: decl.params.clone(),
        r_program: compile_channel_program(&decl.body, Channel::R, &decl.params, param_count,&CompileContext::Image)?,
        g_program: compile_channel_program(&decl.body, Channel::G, &decl.params, param_count,&CompileContext::Image)?,
        b_program: compile_channel_program(&decl.body, Channel::B, &decl.params, param_count,&CompileContext::Image)?,
        a_program: compile_channel_program(&decl.body, Channel::A, &decl.params, param_count,&CompileContext::Image)?,
    })
}

// pub fn compile_effect_decl(decl: &EffectDecl) -> Result<Effect, EngineError> {
//     let mut r_program = Vec::new();
//     let mut g_program = Vec::new();
//     let mut b_program = Vec::new();
//     let mut a_program = Vec::new();
//     let mut t_program = Vec::new();
    

//     // Track local variables active in this filter's scope.
//     // We clone the filter's input parameters into it so they act like local variables!
//     let mut local_scope = decl.params.clone();
//     let mut local_setup_instructions = Vec::new();
//     let param_count = decl.params.len();

//     for statement in &decl.body {
//         match statement {
//             Statement::Let { name, value } => {
//                 // 1. Compile the expression for the local variable
//                 let mut expr_program = compile_expr(value, &local_scope, param_count)?;
//                 local_setup_instructions.append(&mut expr_program);
//                 let local_index = local_scope.len() - param_count;
//                 local_setup_instructions.push(Instruction::StoreLocal(local_index));

//                 local_scope.push(name.clone());
//             }
//             Statement::Channel(ChannelAssign { channel, value }) => {
//                 // Compile the channel value expression using our updated scope
//                 let program = compile_expr(value, &local_scope, param_count)?;

//                 match channel {
//                     Channel::R => r_program = program,
//                     Channel::G => g_program = program,
//                     Channel::B => b_program = program,
//                     Channel::A => a_program = program,
//                     Channel::T => t_program = program,
//                     _ => {}
//                 }
//             }
//         }
//     }

//     // Channels left unassigned pass through unchanged.
//     if r_program.is_empty() {
//         r_program = vec![Instruction::LoadR];
//     }
//     if g_program.is_empty() {
//         g_program = vec![Instruction::LoadG];
//     }
//     if b_program.is_empty() {
//         b_program = vec![Instruction::LoadB];
//     }
//     if a_program.is_empty() {
//         a_program = vec![Instruction::LoadA];
//     }
//     if t_program.is_empty() {
//         t_program = vec![Instruction::LoadT];
//     }

//     // Prepend the local variable calculations to the channel programs!
//     // Every channel executing needs these locals set up first.
//     r_program = [local_setup_instructions.clone(), r_program].concat();
//     g_program = [local_setup_instructions.clone(), g_program].concat();
//     b_program = [local_setup_instructions.clone(), b_program].concat();
//     a_program = [local_setup_instructions.clone(), a_program].concat();
//     t_program = [local_setup_instructions.clone(), t_program].concat();

//     Ok(Effect {
//         name: decl.name.clone(),
//         params: decl.params.clone(),
//         r_program,
//         g_program,
//         b_program,
//         a_program,
//         t_program,
//     })
// }
fn const_number(expr: &Expr) -> Result<f32, EngineError> {
    match expr {
        Expr::Int(v) => Ok(*v as f32),
        Expr::Float(v) => Ok(*v as f32),
        Expr::Neg(inner) => Ok(-const_number(inner)?),
        other => Err(EngineError::Compile(format!(
            "kernel matrix entries must be numeric literals, got {other:?}"
        ))),
    }
}

pub fn compile_kernel_decl(name: &str, matrix: &Expr) -> Result<Kernel, EngineError> {
    let rows = match matrix {
        Expr::Array(rows) => rows,
        _ => {
            return Err(EngineError::Compile(
                "kernel matrix must be an array of arrays".into(),
            ));
        }
    };

    let size = rows.len();
    let mut flat = Vec::with_capacity(size * size);

    for row in rows {
        match row {
            Expr::Array(cells) => {
                if cells.len() != size {
                    return Err(EngineError::Compile("kernel matrix must be square".into()));
                }
                for c in cells {
                    flat.push(const_number(c)?);
                }
            }
            _ => {
                return Err(EngineError::Compile(
                    "kernel matrix rows must be arrays".into(),
                ));
            }
        }
    }

    let sum: f32 = flat.iter().sum();
    let divisor = if sum == 0.0 { 1.0 } else { sum };

    Ok(Kernel {
        name: name.to_string(),
        matrix: flat,
        size,
        divisor,
    })
}

pub struct Engine {
    vars: HashMap<String, Value>,
    filters: HashMap<String, Filter>,
    afilters: HashMap<String, AudioFilter>,
    kernels: HashMap<String, Kernel>,
    imported_files: HashSet<String>,
    effects: HashMap<String, Effect>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            filters: HashMap::new(),
            kernels: HashMap::new(),
            effects: HashMap::new(),
            afilters: HashMap::new(),
            imported_files: HashSet::new(),
        }
    }
    

   
    fn import_file(&mut self, path: &str) -> Result<(), EngineError> {
        if self.imported_files.contains(path) {
            return Ok(());
        }

        self.imported_files.insert(path.to_string());

        let source = std::fs::read_to_string(path)
            .map_err(|e| EngineError::Eval(format!("import failed: {e}")))?;

        let program = crate::parser::parse(&source)
            .map_err(|e| EngineError::Eval(format!("parse failed: {e}")))?;

        self.run(&program)?;

        Ok(())
    }

    pub fn run(&mut self, program: &Program) -> Result<(), EngineError> {
        for item in &program.items {
            self.exec_item(item)?;
        }
        Ok(())
    }

    fn exec_item(&mut self, item: &Item) -> Result<(), EngineError> {
        match item {
            Item::Import(import) => {
                match import {
                    Import::File { path, .. } => {
                        self.import_file(path)?;
                    }

                    Import::Std(path) => {
                        let mut std_path = String::from("stdlib/");

                        std_path
                            .push_str(&path.iter().skip(1).cloned().collect::<Vec<_>>().join("/"));

                        std_path.push_str(".drive");

                        self.import_file(&std_path)?;
                    }
                }

                Ok(())
            }
            Item::AudioFilterDecl(decl) => {
                let afilter = compile_audiofilter_decl(decl)?;
                self.afilters.insert(decl.name.clone(),afilter);
                Ok(())
            }

            Item::FilterDecl(decl) => {
                let filter = compile_filter_decl(decl)?;
                self.filters.insert(decl.name.clone(), filter);
                Ok(())
            }
            // Item::EffectDecl(decl) => {
            //     let effect = compile_effect_decl(decl)?;
            //     self.effects.insert(decl.name.clone(),effect);
            //     Ok(())
            // }
            Item::ForLoop { variable, range, items } => {
                let step_range = self.expr_to_step_range(range)?;
                
                for val in step_range.iter() {
                    // Inject the loop variable into the Engine's global scope!
                    self.vars.insert(variable.clone(), Value::Number(val as f64));
                    
                   
                    for inner_item in items {
                        self.exec_item(inner_item)?;
                    }
                }
                Ok(())
            }
            Item::IfElse { cond, true_branch, false_branch } => {
                let cond_val = self.eval(cond)?;
                let is_true = match cond_val {
                    Value::Number(n) => n != 0.0,
                    _ => return Err(EngineError::EvalError("If condition must be a number".into())),
                };

                let branch_to_run = if is_true { true_branch } else { false_branch };
                for inner_item in branch_to_run {
                    self.exec_item(inner_item)?;
                }
                Ok(())
            }
            Item::Print { args } => {
                let len = args.len();

                let mut str = match &args[0] {
                    Expr::Str(string) => string.clone(),
                    _ => {
                        return Err(EngineError::Eval(
                            "First argument to print must be a String!!".to_string(),
                        ));
                    }
                };

                let mut arg_index = 1;

                while let Some(placeholder_pos) = str.find("{}") {
                    if arg_index >= len {
                        return Err(EngineError::Eval(
                            "Not enough arguments provided for format string!".to_string(),
                        ));
                    }

                    let replacement = match self.eval(&args[arg_index])? {
                        Value::Number(n) => {
                            
                            if n.fract() == 0.0 {
                                (n as i64).to_string()
                            } else {
                                n.to_string()
                            }
                        }
                        Value::String(s) => s,
                        Value::Frame(_) => return Err(EngineError::Eval(
                            "cannot print a Frame".into(),
                        )),
                        Value::Track(_) => return Err(EngineError::Eval("cannot print a Track".into())),
                    };

                    str.replace_range(placeholder_pos..placeholder_pos + 2, &replacement);

                    arg_index += 1;
                }

                if arg_index < len {
                    return Err(EngineError::Eval(
                        "Too many arguments provided for format string!".to_string(),
                    ));
                }

                println!("{}", str);
                Ok(())
            }

            Item::KernelDecl { name, matrix } => {
                let kernel = compile_kernel_decl(name, matrix)?;
                self.kernels.insert(name.clone(), kernel);
                Ok(())
            }

            Item::Assign { name, value } => {
                let v = self.eval(value)?;
                self.vars.insert(name.clone(), v);
                Ok(())
            }

            Item::Export { value, path } => {
                let value = self.eval_export(value)?;
                let path_str = match path {
                    Expr::Str(s) => s.clone(),
                    _ => {
                        return Err(EngineError::Eval(
                            "export path must be a string literal".into(),
                        ));
                    }
                };
                match value{
                    Value::Frame(f) => {io::encode_image(&f, &path_str).map_err(|_| EngineError::Eval("Image Export Failed! Either Frame is Empty or invalid".into()))?;}
                    Value::Track(t) => {
                        let path = Path::new(&path_str);
                        io::encode_wav(&t, path).expect("Encoding Audio failed! Invalid Track");
                    }

                    
                
                    _ => {}
                }
                
                Ok(())
            
        }
    }
}

    // ── Evaluation ──────────────────────────────────────────────────────────

    fn eval(&mut self, expr: &Expr) -> Result<Value, EngineError> {
        match expr {
            Expr::Ident(name) => self
                .vars
                .get(name)
                .cloned()
                .ok_or_else(|| EngineError::UndefinedVar(name.clone())),

            Expr::Int(v) => Ok(Value::Number(*v as f64)),
            Expr::Float(v) => Ok(Value::Number(*v)),

            Expr::Neg(inner) => match self.eval(inner)? {
                Value::Number(n) => Ok(Value::Number(-n)),
                _ => Err(EngineError::Eval("cannot negate a frame".into())),
            },
            Expr::Not(inner) => match self.eval(inner)? {
                Value::Number(n) => Ok(Value::Number(if n == 0.0 { 1.0 } else { 0.0 })),
                _ => Err(EngineError::Eval("cannot apply 'not' to a frame".into())),
            },
            Expr::Str(s) => Ok(Value::String(s.clone())),

            Expr::BinOp { op, lhs, rhs } => {
                let l = self.eval_number(lhs)?;
                let r = self.eval_number(rhs)?;
                Ok(Value::Number(match op {
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => {
                        if r == 0.0 {
                            0.0
                        } else {
                            l / r
                        }
                    }
                    BinOp::Eq => {
                        if l == r {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    BinOp::Ne => {
                        if l != r {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    BinOp::Gt => {
                        if l > r {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    BinOp::Ge => {
                        if l >= r {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    BinOp::Lt => {
                        if l < r {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    BinOp::Le => {
                        if l <= r {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    BinOp::And => {
                        if l != 0.0 && r != 0.0 {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    BinOp::Or => {
                        if l != 0.0 || r != 0.0 {
                            1.0
                        } else {
                            0.0
                        }
                    }
                }))
            }

            Expr::Call { path, args } => self.eval_call(path, args),

            Expr::Pipe { base, stages } => {
                
                match self.eval(base)? {
                    Value::Frame(mut frame) => {
                        let mut pipeline = EffectPipeline { operations: Vec::new() };
                        for stage in stages {
                            pipeline.operations.push(self.compile_stage(stage)?);
                        }
                        pipeline.execute(&mut frame)?;
                        Ok(Value::Frame(frame))
                    }
                    Value::Track(mut track) => {
                        let mut pipeline = AudioPipeline { operations: Vec::new() };
                        for stage in stages {
                            pipeline.operations.push(self.compile_audio(stage)?);
                        }
                        pipeline.execute(&mut track)?;
                        Ok(Value::Track(track))
                    }
                    _ => Err(EngineError::Eval(
                        "Piping '->' is only supported on Frames and Tracks!".into(),
                    )),
                }
            }

            other => Err(EngineError::Eval(format!(
                "cannot evaluate expression: {other:?}"
            ))),
        }
    }

    fn eval_call(&mut self, path: &[String], args: &[Expr]) -> Result<Value, EngineError> {
        let name = path.last().map(String::as_str).unwrap_or("");
        match name {
            "frame" => {
                let path_str = match args.first() {
                    Some(Expr::Str(s)) => s.clone(),
                    _ => return Err(EngineError::Eval("load() requires a string path".into())),
                };
                
                let frame = io::load_image(&path_str, "rgba")
                    .map_err(|e| EngineError::Eval(format!("{e}")))?;
                Ok(Value::Frame(frame))
            }
            "track" => {
                let path_str = match args.first(){
                    Some(Expr::Str(s)) => s.clone(),
                    _=> return Err(EngineError::Eval("track() requires a string path".into())),
                };
                let path = Path::new(&path_str);
                let track = io::decode_audio(path).map_err(|_| EngineError::Eval(format!("Audio Decoding Failed, Check Path....")))?;
                Ok(Value::Track(track))
            }
            "text" => {
                if args.len() < 6 {
                    return Err(EngineError::EvalError("text() needs 6 args: str,font_path, size, r, g, b,".into()));
                }
                
                
                let text_content = match &args[0] {
                    Expr::Str(s) => s.clone(), 
                    _ => return Err(EngineError::EvalError("First arg to text() must be string".into())),
                };
                let font_path= self.eval_string(&args[1])?;
                
                
                let size = self.eval_number(&args[2])? as f32;
                let r = self.eval_number(&args[3])? as u8;
                let g = self.eval_number(&args[4])? as u8;
                let b = self.eval_number(&args[5])? as u8;
                let bytes = match  std::fs::read(font_path){
                    Ok(b) => b,
                    Err(_) => return Err(EngineError::Eval("Invalid Font Path".into())),
                };
                let font = Font::from_bytes(bytes, FontSettings::default()).expect("Font Loading Failed, check path");

                
                
                
                
                
                let mut txt_obj = Text::new(&text_content, font,size, Pos(0,0),Color::RGBA(r, g, b,255));
                let frame = txt_obj.picturize().expect("Text Conversion Failed!");
                
                Ok(Value::Frame(frame))
            }
            
            "blank" => {
                if args.len() != 2 {
                    return Err(EngineError::Eval(
                        "blank() requires width and height".into(),
                    ));
                }

                let width = self.eval_number(&args[0])? as u32;
                let height = self.eval_number(&args[1])? as u32;

                Ok(Value::Frame(Frame::blank(width, height)))
            }
            "silence" => {
                if args.len() != 3 {
                    return Err(EngineError::Eval(
                        "silence() requires Duration, sample rate  and channel ".into(),
                    ));
                }

                let dur = self.eval_number(&args[0])? ;
                let duration = TimeStamp::from_seconds(dur, 1, 44100);
                let sample_rate = self.eval_number(&args[1])? as u32;
                let channel = self.eval_number(&args[2])? as u16;


                Ok(Value::Track(Track::silence(duration, sample_rate,channel)))
            }

            other => Err(EngineError::UndefinedOp(format!(
                "unknown function '{other}'"
            ))),
        }
    }

    fn eval_frame(&mut self, expr: &Expr) -> Result<Frame, EngineError> {
        match self.eval(expr)? {
            Value::Frame(f) => Ok(f),
            Value::Number(_) => Err(EngineError::Eval("expected a frame, got a number".into())),
            Value::String(_) => Err(EngineError::Eval("expectd a frame found string".into())),
            Value::Track(_) => Err(EngineError::Eval(("expected a frame found track".into()))),
        }
    }
    fn eval_export(&mut self, expr: &Expr) -> Result<Value, EngineError> {
        match self.eval(expr)? {
            Value::Track(t) => Ok(Value::Track(t)),
            Value::Number(_) => Err(EngineError::Eval("expected a track/frame, got a number".into())),
            Value::String(_) => Err(EngineError::Eval("expectd a track/frame found string".into())),
            Value::Frame(f) => Ok(Value::Frame(f)),
        }
    }

    fn eval_track(&mut self, expr: &Expr) -> Result<Track, EngineError> {
        match self.eval(expr)? {
            Value::Track(f) => Ok(f),
            Value::Number(_) => Err(EngineError::Eval("expected a track, got a number".into())),
            Value::String(_) => Err(EngineError::Eval("expectd a track found string".into())),
            Value::Frame(_) => Err(EngineError::Eval(("expected a track found frame".into()))),
        }
    }

    fn eval_number(&mut self, expr: &Expr) -> Result<f64, EngineError> {
        match self.eval(expr)? {
            Value::Number(n) => Ok(n),
            Value::Frame(_) => Err(EngineError::Eval("expected a number, got a frame".into())),
            Value::String(_) => Err(EngineError::Eval("expectd a number found string".into())),
            Value::Track(_) => Err(EngineError::Eval(("expected a number found track".into())))
        }
    }
    fn eval_string(&mut self, expr: &Expr) -> Result<String, EngineError> {
        let val = self.eval(expr)?;
        match val {
            Value::String(s) => Ok(s),
            _ => Err(EngineError::EvalError("Expected a string! Did you pass a number to a spelling bee?".into())),
        }
    }

    fn eval_usize(&mut self, expr: &Expr) -> Result<usize, EngineError> {
        let n = self.eval_number(expr)?;
        if n < 0.0 {
            return Err(EngineError::Eval("range bound must be non-negative".into()));
        }
        Ok(n as usize)
    }
    fn compile_audio(&mut self, stage: &crate::parser::PipeStage) -> Result<AudioOperation, EngineError> {
        let name = stage
            .path
            .last()
            .ok_or_else(|| EngineError::Eval("Empty pipeline stage".into()))?;

        let mut params = Vec::new();

        for arg in &stage.args {
            match self.eval(arg)? {
                Value::Number(v) => params.push(v as f32),
                _ => {
                    return Err(EngineError::Eval(
                        format!("Audio filter '{}' only accepts numeric parameters.", name)
                    ));
                }
            }
        }
        

        let filter = self
            .afilters
            .get(name)
            .ok_or_else(|| EngineError::Eval(format!("Unknown audio filter '{}'", name)))?;

        Ok(AudioOperation::PointFilter {
            filter: filter.clone(),
            params,
        })
    }

    fn compile_stage(
        &mut self,
        stage: &crate::parser::PipeStage,
    ) -> Result<Operation, EngineError> {
        let name = stage
            .path
            .last()
            .ok_or_else(|| EngineError::Compile("empty stage path".into()))?;

        if name.as_str() == "resize" {
            if stage.args.len() != 2 {
                return Err(EngineError::Compile("resize requires exactly 2 arguments: (width, height)".into()));
            }
            let width = self.eval_number(&stage.args[0])?.max(1.0) as u32;
            let height = self.eval_number(&stage.args[1])?.max(1.0) as u32;
            return Ok(Operation::NativeResize { width, height });
        }
        
        // Let users crop safely: `img -> crop(10, 10, 500, 500)`
        if name.as_str() == "crop" {
            if stage.args.len() != 4 {
                return Err(EngineError::Compile("crop requires exactly 4 arguments: (x, y, width, height)".into()));
            }
            let x = self.eval_number(&stage.args[0])?.max(0.0) as u32;
            let y = self.eval_number(&stage.args[1])?.max(0.0) as u32;
            let width = self.eval_number(&stage.args[2])?.max(1.0) as u32;
            let height = self.eval_number(&stage.args[3])?.min(1.0) as u32;
            return Ok(Operation::NativeCrop { x, y, width, height });
        }
        if name.as_str() == "blend" {
            if stage.args.len() != 4 {
                return Err(EngineError::Compile("blend requires exactly 4 arguments: (x, y, frame, alpha)".into()));
            }
            let x = self.eval_number(&stage.args[0])?.max(0.0) as u32;
            let y = self.eval_number(&stage.args[1])?.max(0.0) as u32;
            let frame = self.eval_frame(&stage.args[2]).expect("Not a valid Frame.");
            let alpha = self.eval_number(&stage.args[3])?.max(0.0) ;
            return Ok(Operation::Blend { x, y, frame2: frame, alpha });
        }
        

        let mask = match &stage.mask {
            Some((x_range, y_range)) => Some(self.build_mask(x_range, y_range)?),
            None => None,
        };

        if let Some(filter) = self.filters.get(name.as_str()).cloned() {
            let mut params = Vec::with_capacity(stage.args.len());
            for arg in &stage.args {
                params.push(self.eval_number(arg)? as f32);
            }
            return Ok(Operation::PointFilter {
                filter,
                params,
                mask,
            });
        }

        // --- NEW: THE BACKDOOR INTERCEPT ---
        // If the operation is called "blur" and they actually provided a number argument...
        if name.as_str() == "blur" && !stage.args.is_empty() {
            // Read their argument (e.g. blur(15) -> 15.0)
            let size_f64 = self.eval_number(&stage.args[0])?;
            // Ensure size is at least 1 so we don't accidentally divide by zero later!
            let size = size_f64.max(1.0) as usize;

            // Spawn the custom matrix right out of thin air
            let dynamic_kernel = Kernel::generate_blur("blur", size);

            // Bypass the static dictionary entirely and ship it to the pipeline
            return Ok(Operation::Convolution {
                kernel: dynamic_kernel,
                mask,
            });
        }
        // --- END OF BACKDOOR ---

        if let Some(kernel) = self.kernels.get(name.as_str()).cloned() {
            return Ok(Operation::Convolution { kernel, mask });
        }

        Err(EngineError::UndefinedOp(format!(
            "unknown filter or kernel '{name}'"
        )))
    }

    fn build_mask(&mut self, x_range: &Expr, y_range: &Expr) -> Result<Mask, EngineError> {
        let x = self.expr_to_step_range(x_range)?;
        let y = self.expr_to_step_range(y_range)?;
        Ok(Mask::Rect(Rect { x, y }))
    }

    fn expr_to_step_range(&mut self, expr: &Expr) -> Result<StepRange, EngineError> {
        match expr {
            Expr::Range { start, end, step } => {
                let s = self.eval_usize(start)?;
                let e = self.eval_usize(end)?;
                let step = match step {
                    Some(step_expr) => self.eval_usize(step_expr)?.max(1),
                    None => 1,
                };
                Ok(StepRange { range: s..e, step })
            }
            other => {
                let v = self.eval_usize(other)?;
                Ok(StepRange {
                    range: v..(v + 1),
                    step: 1,
                })
            }
        }
    }
}
