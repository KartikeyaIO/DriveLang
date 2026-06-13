use std::collections::{HashMap, HashSet};

use crate::filter::{Filter, Instruction};
use crate::io::io::{self, IOError};
use crate::media::frame::Frame;
use crate::parser::{BinOp, Channel, ChannelAssign, Expr, FilterDecl, Import, Item, Program};
use crate::pipeline::kernel::Kernel;
use crate::pipeline::pipeline::{EffectPipeline, Operation, Pipeline, PipelineError};
use crate::range::{Mask, Rect, StepRange};

#[derive(Debug, Clone)]
pub enum Value {
    Frame(Frame),
    Number(f64),
}

#[derive(Debug)]
pub enum EngineError {
    Compile(String),
    Eval(String),
    Pipeline(PipelineError),
    Io(IOError),
    UndefinedVar(String),
    UndefinedOp(String),
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

// ─────────────────────────────────────────────────────────────────────────
// Expression -> bytecode compiler (used for filter() bodies)
// ─────────────────────────────────────────────────────────────────────────

fn compile_expr(expr: &Expr, params: &[String]) -> Result<Vec<Instruction>, EngineError> {
    let mut out = Vec::new();
    compile_into(expr, params, &mut out)?;
    Ok(out)
}

fn compile_into(
    expr: &Expr,
    params: &[String],
    out: &mut Vec<Instruction>,
) -> Result<(), EngineError> {
    match expr {
        Expr::Int(v) => out.push(Instruction::PushInt(*v)),
        Expr::Float(v) => out.push(Instruction::PushFloat(*v as f32)),

        Expr::Ident(name) => match name.as_str() {
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
                    out.push(Instruction::LoadParam(idx));
                } else {
                    return Err(EngineError::Compile(format!(
                        "unknown identifier '{other}' in filter body"
                    )));
                }
            }
        },

        Expr::Neg(inner) => {
            compile_into(inner, params, out)?;
            out.push(Instruction::Neg);
        }

        Expr::BinOp { op, lhs, rhs } => {
            compile_into(lhs, params, out)?;
            compile_into(rhs, params, out)?;
            out.push(match op {
                BinOp::Add => Instruction::Add,
                BinOp::Sub => Instruction::Sub,
                BinOp::Mul => Instruction::Mul,
                BinOp::Div => Instruction::Div,
            });
        }

        Expr::Call { path, args } => {
            let name = path
                .last()
                .ok_or_else(|| EngineError::Compile("empty call path".into()))?;

            // multi-arg builtins handled specially
            match (name.as_str(), args.len()) {
                ("clamp", 3) => {
                    compile_into(&args[0], params, out)?;
                    compile_into(&args[1], params, out)?;
                    compile_into(&args[2], params, out)?;
                    out.push(Instruction::Clamp);
                    return Ok(());
                }
                ("min", 2) => {
                    compile_into(&args[0], params, out)?;
                    compile_into(&args[1], params, out)?;
                    out.push(Instruction::Min);
                    return Ok(());
                }
                ("max", 2) => {
                    compile_into(&args[0], params, out)?;
                    compile_into(&args[1], params, out)?;
                    out.push(Instruction::Max);
                    return Ok(());
                }
                ("pow", 2) => {
                    compile_into(&args[0], params, out)?;
                    compile_into(&args[1], params, out)?;
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
            compile_into(&args[0], params, out)?;
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

pub fn compile_filter_decl(decl: &FilterDecl) -> Result<Filter, EngineError> {
    let mut r_program = Vec::new();
    let mut g_program = Vec::new();
    let mut b_program = Vec::new();
    let mut a_program = Vec::new();

    for ChannelAssign { channel, value } in &decl.body {
        let program = compile_expr(value, &decl.params)?;
        match channel {
            Channel::R => r_program = program,
            Channel::G => g_program = program,
            Channel::B => b_program = program,
            Channel::A => a_program = program,
        }
    }

    // Channels left unassigned pass through unchanged.
    if r_program.is_empty() {
        r_program = vec![Instruction::LoadR];
    }
    if g_program.is_empty() {
        g_program = vec![Instruction::LoadG];
    }
    if b_program.is_empty() {
        b_program = vec![Instruction::LoadB];
    }
    if a_program.is_empty() {
        a_program = vec![Instruction::LoadA];
    }

    Ok(Filter {
        name: decl.name.clone(),
        params: decl.params.clone(),
        r_program,
        g_program,
        b_program,
        a_program,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Kernel matrix compiler: kernel name = [[..],[..],[..]];
// ─────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────
// Interpreter
// ─────────────────────────────────────────────────────────────────────────

pub struct Engine {
    vars: HashMap<String, Value>,
    filters: HashMap<String, Filter>,
    kernels: HashMap<String, Kernel>,
    imported_files: HashSet<String>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            filters: HashMap::new(),
            kernels: HashMap::new(),
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

                        std_path.push_str(".edt");

                        self.import_file(&std_path)?;
                    }
                }

                Ok(())
            }

            Item::FilterDecl(decl) => {
                let filter = compile_filter_decl(decl)?;
                self.filters.insert(decl.name.clone(), filter);
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
                let frame = self.eval_frame(value)?;
                let path_str = match path {
                    Expr::Str(s) => s.clone(),
                    _ => {
                        return Err(EngineError::Eval(
                            "export path must be a string literal".into(),
                        ));
                    }
                };
                io::encode_image(&frame, &path_str)?;
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
                }))
            }

            Expr::Call { path, args } => self.eval_call(path, args),

            Expr::Pipe { base, stages } => {
                let mut frame = self.eval_frame(base)?;
                let mut pipeline = EffectPipeline {
                    operations: Vec::new(),
                };

                for stage in stages {
                    let op = self.compile_stage(stage)?;
                    pipeline.operations.push(op);
                }

                pipeline.execute(&mut frame)?;
                Ok(Value::Frame(frame))
            }

            other => Err(EngineError::Eval(format!(
                "cannot evaluate expression: {other:?}"
            ))),
        }
    }

    fn eval_call(&mut self, path: &[String], args: &[Expr]) -> Result<Value, EngineError> {
        let name = path.last().map(String::as_str).unwrap_or("");
        match name {
            "load" => {
                let path_str = match args.first() {
                    Some(Expr::Str(s)) => s.clone(),
                    _ => return Err(EngineError::Eval("load() requires a string path".into())),
                };
                // Always load as RGBA — pipeline normalizes anyway, and this
                // keeps load() side-effect-free w.r.t. format.
                let frame = io::load_image(&path_str, "rgba")
                    .map_err(|e| EngineError::Eval(format!("{e}")))?;
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
            other => Err(EngineError::UndefinedOp(format!(
                "unknown function '{other}'"
            ))),
        }
    }

    fn eval_frame(&mut self, expr: &Expr) -> Result<Frame, EngineError> {
        match self.eval(expr)? {
            Value::Frame(f) => Ok(f),
            Value::Number(_) => Err(EngineError::Eval("expected a frame, got a number".into())),
        }
    }

    fn eval_number(&mut self, expr: &Expr) -> Result<f64, EngineError> {
        match self.eval(expr)? {
            Value::Number(n) => Ok(n),
            Value::Frame(_) => Err(EngineError::Eval("expected a number, got a frame".into())),
        }
    }

    fn eval_usize(&mut self, expr: &Expr) -> Result<usize, EngineError> {
        let n = self.eval_number(expr)?;
        if n < 0.0 {
            return Err(EngineError::Eval("range bound must be non-negative".into()));
        }
        Ok(n as usize)
    }

    // ── Pipe stage -> Operation ────────────────────────────────────────────

    fn compile_stage(
        &mut self,
        stage: &crate::parser::PipeStage,
    ) -> Result<Operation, EngineError> {
        let name = stage
            .path
            .last()
            .ok_or_else(|| EngineError::Compile("empty stage path".into()))?;

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
