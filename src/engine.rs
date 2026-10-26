use fontdue::{Font, FontSettings};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::filter::{AudioFilter, Effect, Filter, Instruction};
use crate::io::io::{self, IOError};
use crate::media::folder::Folder;
use crate::media::frame::{Color, Frame, Pos};
use crate::media::track::Track;
use crate::media::video::TimeStamp;
use crate::parser::{
    AudioFilterDecl, BinOp, Channel, ChannelAssign, EffectDecl, Expr, FStringSegment,
    FilterDecl, FnDecl, ForIterable, Import, Item, PipeStage, Program, Statement,
};
use crate::pipeline::kernel::Kernel;
use crate::pipeline::pipeline::{
    AudioOperation, AudioPipeline, EffectPipeline, Operation, Pipeline, PipelineError,
};
use crate::range::{Mask, Rect, StepRange};
use crate::text::Text;

fn is_stdlib_dir(path: &Path) -> bool {
    path.is_dir()
        && ["core", "color", "advanced", "audio", "kernels"]
            .iter()
            .all(|module| path.join(format!("{module}.drive")).is_file())
}

fn find_stdlib() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    // An explicit override always wins when it points to a complete library.
    if let Some(dir) = std::env::var_os("DRIVELANG_STDLIB") {
        candidates.push(PathBuf::from(dir));
    }

    // Portable release archives keep stdlib next to drive. Also support
    // conventional bin/ + share/ layouts used by system package managers.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("stdlib"));
            if let Some(prefix) = parent.parent() {
                candidates.push(prefix.join("share/DriveLang/stdlib"));
                candidates.push(prefix.join("share/drivelang/stdlib"));
            }
        }

        // Development and local cargo installs may live under target/ or
        // ~/.cargo/bin. Walk their parents for a repository/package layout.
        for ancestor in exe.ancestors().skip(1) {
            candidates.push(ancestor.join("stdlib"));
        }
    }

    // A project-local library should be found when the installed executable
    // is run from a checkout, even if an older user-level copy also exists.
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            candidates.push(ancestor.join("stdlib"));
        }
    }

    // `cargo install --path .` builds from the local project. Retain that
    // source-tree location as a fallback when the binary is run elsewhere.
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib"));

    // User data locations on Linux/macOS and Windows.
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        let data_home = PathBuf::from(data_home);
        candidates.push(data_home.join("DriveLang/stdlib"));
        candidates.push(data_home.join("drivelang/stdlib"));
    }
    for variable in ["APPDATA", "LOCALAPPDATA", "PROGRAMDATA"] {
        if let Some(base) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(&base).join("DriveLang/stdlib"));
            candidates.push(PathBuf::from(base).join("drivelang/stdlib"));
        }
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        candidates.push(home.join(".local/share/DriveLang/stdlib"));
        candidates.push(home.join(".local/share/drivelang/stdlib"));
        candidates.push(home.join("AppData/Roaming/DriveLang/stdlib"));
        candidates.push(home.join("DriveLang/stdlib"));
    }

    // Conventional Unix-wide installs.
    for dir in [
        "/usr/local/share/DriveLang/stdlib",
        "/usr/local/share/drivelang/stdlib",
        "/usr/share/DriveLang/stdlib",
        "/usr/share/drivelang/stdlib",
        "/opt/DriveLang/stdlib",
        "/opt/drivelang/stdlib",
    ] {
        candidates.push(PathBuf::from(dir));
    }

    candidates.into_iter().find(|path| is_stdlib_dir(path))
}

#[derive(Debug, Clone)]
pub enum Value {
    Frame(Frame),
    Track(Track),
    Folder(Folder),
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
impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Compile(s) => write!(f, "CompilationFailed: {s}"),
            EngineError::Eval(e) => write!(f, "{e}"),
            EngineError::EvalError(s) => write!(f, "{s}"),
            EngineError::Io(e) => write!(f, "{e}"),
            EngineError::Pipeline(e) => write!(f, "{e}"),
            EngineError::UndefinedOp(e) => write!(f, "UndefinedOperation: {e}"),
            EngineError::UndefinedVar(e) => write!(f, "UndefinedVariable: {e}"),
        }
    }
}

/// Compiler Context
enum CompileContext {
    Image,
    Audio,
    Effect,
}

fn substitute_params_in_expr(expr: &Expr, params: &[String], call_prefix: &str) -> Expr {
    match expr {
        Expr::Ident(name) => {
            if params.contains(name) {
                Expr::Ident(format!("{}{}", call_prefix, name))
            } else {
                Expr::Ident(name.clone())
            }
        }
        Expr::Neg(inner) => Expr::Neg(Box::new(substitute_params_in_expr(inner, params, call_prefix))),
        Expr::Not(inner) => Expr::Not(Box::new(substitute_params_in_expr(inner, params, call_prefix))),
        Expr::BinOp { op, lhs, rhs } => Expr::BinOp {
            op: op.clone(),
            lhs: Box::new(substitute_params_in_expr(lhs, params, call_prefix)),
            rhs: Box::new(substitute_params_in_expr(rhs, params, call_prefix)),
        },
        Expr::Call { path, args } => Expr::Call {
            path: path.clone(),
            args: args
                .iter()
                .map(|a| substitute_params_in_expr(a, params, call_prefix))
                .collect(),
        },
        Expr::Index { base, key } => Expr::Index {
            base: Box::new(substitute_params_in_expr(base, params, call_prefix)),
            key: Box::new(substitute_params_in_expr(key, params, call_prefix)),
        },
        Expr::FieldAccess { base, field } => Expr::FieldAccess {
            base: Box::new(substitute_params_in_expr(base, params, call_prefix)),
            field: field.clone(),
        },
        Expr::MethodCall { base, method, args } => Expr::MethodCall {
            base: Box::new(substitute_params_in_expr(base, params, call_prefix)),
            method: method.clone(),
            args: args
                .iter()
                .map(|a| substitute_params_in_expr(a, params, call_prefix))
                .collect(),
        },
        Expr::Range { start, end, step } => Expr::Range {
            start: Box::new(substitute_params_in_expr(start, params, call_prefix)),
            end: Box::new(substitute_params_in_expr(end, params, call_prefix)),
            step: step
                .as_ref()
                .map(|s| Box::new(substitute_params_in_expr(s, params, call_prefix))),
        },
        other => other.clone(),
    }
}

fn collect_declared_names(stmts: &[Statement], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Statement::Let { name, .. } => {
                if !out.contains(name) {
                    out.push(name.clone());
                }
            }
            Statement::IfElse {
                true_branch,
                false_branch,
                ..
            } => {
                collect_declared_names(true_branch, out);
                collect_declared_names(false_branch, out);
            }
            Statement::ForLoop { variable, body, .. } => {
                if !out.contains(variable) {
                    out.push(variable.clone());
                }
                collect_declared_names(body, out);
            }
            _ => {}
        }
    }
}

fn substitute_params_in_stmts(
    stmts: &[Statement],
    params: &[String],
    call_prefix: &str,
) -> Vec<Statement> {
    let mut scoped_names = params.to_vec();
    collect_declared_names(stmts, &mut scoped_names);
    substitute_scoped_stmts(stmts, &scoped_names, call_prefix)
}

fn substitute_scoped_stmts(
    stmts: &[Statement],
    scoped_names: &[String],
    call_prefix: &str,
) -> Vec<Statement> {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Statement::Let { name, value } => Statement::Let {
                name: format!("{}{}", call_prefix, name),
                value: substitute_params_in_expr(value, scoped_names, call_prefix),
            },
            Statement::Channel(ChannelAssign { channel, value }) => {
                Statement::Channel(ChannelAssign {
                    channel: channel.clone(),
                    value: substitute_params_in_expr(value, scoped_names, call_prefix),
                })
            }
            Statement::IfElse {
                cond,
                true_branch,
                false_branch,
            } => Statement::IfElse {
                cond: Box::new(substitute_params_in_expr(cond, scoped_names, call_prefix)),
                true_branch: substitute_scoped_stmts(true_branch, scoped_names, call_prefix),
                false_branch: substitute_scoped_stmts(false_branch, scoped_names, call_prefix),
            },
            Statement::FilterCall { name, args } => Statement::FilterCall {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|a| substitute_params_in_expr(a, scoped_names, call_prefix))
                    .collect(),
            },
            Statement::ForLoop {
                variable,
                start,
                end,
                step,
                body,
            } => Statement::ForLoop {
                variable: format!("{}{}", call_prefix, variable),
                start: Box::new(substitute_params_in_expr(start, scoped_names, call_prefix)),
                end: Box::new(substitute_params_in_expr(end, scoped_names, call_prefix)),
                step: step
                    .as_ref()
                    .map(|s| Box::new(substitute_params_in_expr(s, scoped_names, call_prefix))),
                body: substitute_scoped_stmts(body, scoped_names, call_prefix),
            },
            Statement::Return(expr) => {
                Statement::Return(substitute_params_in_expr(expr, scoped_names, call_prefix))
            }
        })
        .collect()
}

fn eval_const_math_expr(
    expr: &Expr,
    active_vars: &HashMap<String, Value>,
) -> Result<f64, EngineError> {
    match expr {
        Expr::Int(v) => Ok(*v as f64),
        Expr::Float(v) => Ok(*v),
        Expr::Neg(inner) => Ok(-eval_const_math_expr(inner, active_vars)?),
        Expr::BinOp { op, lhs, rhs } => {
            let l = eval_const_math_expr(lhs, active_vars)?;
            let r = eval_const_math_expr(rhs, active_vars)?;
            Ok(match op {
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
                _ => 0.0,
            })
        }
        Expr::Ident(name) => {
            if let Some(Value::Number(n)) = active_vars.get(name) {
                Ok(*n)
            } else {
                Err(EngineError::Compile(format!(
                    "loop bound identifier '{name}' must be a known numeric variable"
                )))
            }
        }
        Expr::Call { path, args } => {
            let name = path.last().map(String::as_str).unwrap_or("");
            match (name, args.len()) {
                ("abs", 1) => Ok(eval_const_math_expr(&args[0], active_vars)?.abs()),
                ("sqrt", 1) => Ok(eval_const_math_expr(&args[0], active_vars)?.sqrt()),
                ("floor", 1) => Ok(eval_const_math_expr(&args[0], active_vars)?.floor()),
                ("ceil", 1) => Ok(eval_const_math_expr(&args[0], active_vars)?.ceil()),
                ("round", 1) => Ok(eval_const_math_expr(&args[0], active_vars)?.round()),
                ("min", 2) => {
                    let a = eval_const_math_expr(&args[0], active_vars)?;
                    let b = eval_const_math_expr(&args[1], active_vars)?;
                    Ok(a.min(b))
                }
                ("max", 2) => {
                    let a = eval_const_math_expr(&args[0], active_vars)?;
                    let b = eval_const_math_expr(&args[1], active_vars)?;
                    Ok(a.max(b))
                }
                _ => Err(EngineError::Compile(format!(
                    "unsupported function '{name}' in compile-time loop bound"
                ))),
            }
        }
        _ => Err(EngineError::Compile(
            "loop bounds inside filter must be compile-time numbers or known variables".into(),
        )),
    }
}

fn compile_stmts_for_channel(
    stmts: &[Statement],
    target: &Channel,
    params: &[String],
    param_count: usize,
    local_scope: &mut Vec<String>,
    out: &mut Vec<Instruction>,
    context: &CompileContext,
    filter_decls: &HashMap<String, FilterDecl>,
    afilter_decls: &HashMap<String, AudioFilterDecl>,
    active_vars: &HashMap<String, Value>,
) -> Result<(), EngineError> {
    for stmt in stmts {
        match stmt {
            Statement::Let { name, value } => {
                compile_into(
                    value,
                    local_scope,
                    param_count,
                    out,
                    context,
                    filter_decls,
                    afilter_decls,
                    active_vars,
                )?;
                if let Some(pos) = local_scope[param_count..].iter().position(|p| p == name) {
                    out.push(Instruction::StoreLocal(pos));
                } else {
                    let local_index = local_scope.len() - param_count;
                    out.push(Instruction::StoreLocal(local_index));
                    local_scope.push(name.clone());
                }
            }

            Statement::Channel(ChannelAssign { channel, value }) => {
                if channel == target {
                    compile_into(
                        value,
                        local_scope,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    let cur_var = match target {
                        Channel::R => "__cur_r",
                        Channel::G => "__cur_g",
                        Channel::B => "__cur_b",
                        Channel::A => "__cur_a",
                        Channel::T => "__cur_t",
                        Channel::L => "__cur_l",
                    };
                    if let Some(pos) = local_scope[param_count..].iter().position(|p| p == cur_var) {
                        out.push(Instruction::StoreLocal(pos));
                    }
                }
            }

            Statement::IfElse {
                cond,
                true_branch,
                false_branch,
            } => {
                compile_into(
                    cond,
                    local_scope,
                    param_count,
                    out,
                    context,
                    filter_decls,
                    afilter_decls,
                    active_vars,
                )?;

                let jif_idx = out.len();
                out.push(Instruction::JumpIfFalse(0));

                let mut true_scope = local_scope.clone();
                compile_stmts_for_channel(
                    true_branch,
                    target,
                    params,
                    param_count,
                    &mut true_scope,
                    out,
                    context,
                    filter_decls,
                    afilter_decls,
                    active_vars,
                )?;

                let jump_idx = out.len();
                out.push(Instruction::Jump(0));

                out[jif_idx] = Instruction::JumpIfFalse(out.len());

                let mut false_scope = local_scope.clone();
                compile_stmts_for_channel(
                    false_branch,
                    target,
                    params,
                    param_count,
                    &mut false_scope,
                    out,
                    context,
                    filter_decls,
                    afilter_decls,
                    active_vars,
                )?;

                out[jump_idx] = Instruction::Jump(out.len());
            }

            Statement::FilterCall { name, args } => {
                // Feature 1: Filter composition inside filter bodies
                if let Some(callee) = filter_decls.get(name) {
                    let prefix = format!("__call_{}_{}_", name, out.len());
                    for (i, param_name) in callee.params.iter().enumerate() {
                        if let Some(arg) = args.get(i) {
                            compile_into(
                                arg,
                                local_scope,
                                param_count,
                                out,
                                context,
                                filter_decls,
                                afilter_decls,
                                active_vars,
                            )?;
                            let local_idx = local_scope.len() - param_count;
                            out.push(Instruction::StoreLocal(local_idx));
                            local_scope.push(format!("{}{}", prefix, param_name));
                        }
                    }
                    let inlined_body =
                        substitute_params_in_stmts(&callee.body, &callee.params, &prefix);
                    compile_stmts_for_channel(
                        &inlined_body,
                        target,
                        params,
                        param_count,
                        local_scope,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                } else if let Some(callee) = afilter_decls.get(name) {
                    let prefix = format!("__call_{}_{}_", name, out.len());
                    for (i, param_name) in callee.params.iter().enumerate() {
                        if let Some(arg) = args.get(i) {
                            compile_into(
                                arg,
                                local_scope,
                                param_count,
                                out,
                                context,
                                filter_decls,
                                afilter_decls,
                                active_vars,
                            )?;
                            let local_idx = local_scope.len() - param_count;
                            out.push(Instruction::StoreLocal(local_idx));
                            local_scope.push(format!("{}{}", prefix, param_name));
                        }
                    }
                    let inlined_body =
                        substitute_params_in_stmts(&callee.body, &callee.params, &prefix);
                    compile_stmts_for_channel(
                        &inlined_body,
                        target,
                        params,
                        param_count,
                        local_scope,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                } else {
                    return Err(EngineError::Compile(format!(
                        "called filter '{name}' was not declared before this call"
                    )));
                }
            }

            Statement::ForLoop {
                variable,
                start,
                end,
                step,
                body,
            } => {
                // Feature 12: Loop unrolling inside filter bodies
                let s = eval_const_math_expr(start, active_vars)?;
                let e = eval_const_math_expr(end, active_vars)?;
                let st = match step {
                    Some(st_expr) => eval_const_math_expr(st_expr, active_vars)?.max(1.0),
                    None => 1.0,
                };
                let mut iter_val = s;
                while iter_val < e {
                    let mut iter_scope = local_scope.clone();
                    out.push(Instruction::PushFloat(iter_val as f32));
                    let local_index = iter_scope.len() - param_count;
                    out.push(Instruction::StoreLocal(local_index));
                    iter_scope.push(variable.clone());

                    compile_stmts_for_channel(
                        body,
                        target,
                        params,
                        param_count,
                        &mut iter_scope,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    iter_val += st;
                }
            }

            Statement::Return(expr) => {
                compile_into(
                    expr,
                    local_scope,
                    param_count,
                    out,
                    context,
                    filter_decls,
                    afilter_decls,
                    active_vars,
                )?;
            }
        }
    }
    Ok(())
}

fn body_assigns_channel(stmts: &[Statement], target: &Channel) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Statement::Channel(ca) => &ca.channel == target,
        Statement::IfElse { true_branch, false_branch, .. } => {
            body_assigns_channel(true_branch, target) || body_assigns_channel(false_branch, target)
        }
        Statement::ForLoop { body, .. } => body_assigns_channel(body, target),
        Statement::FilterCall { .. } => true,
        _ => false,
    })
}

fn compile_channel_program(
    body: &[Statement],
    target: Channel,
    params: &[String],
    param_count: usize,
    context: &CompileContext,
    filter_decls: &HashMap<String, FilterDecl>,
    afilter_decls: &HashMap<String, AudioFilterDecl>,
    active_vars: &HashMap<String, Value>,
) -> Result<Vec<Instruction>, EngineError> {
    let mut out = Vec::new();
    let mut local_scope: Vec<String> = params.to_vec();

    let cur_var = match target {
        Channel::R => "__cur_r",
        Channel::G => "__cur_g",
        Channel::B => "__cur_b",
        Channel::A => "__cur_a",
        Channel::T => "__cur_t",
        Channel::L => "__cur_l",
    };

    let assigns = body_assigns_channel(body, &target);
    if !assigns {
        // Local lets are compiled independently for each output channel. If
        // this channel is untouched, those locals leave no channel value on
        // the VM stack; explicitly pass the original channel through instead.
        return Ok(vec![match target {
            Channel::R => Instruction::LoadR,
            Channel::G => Instruction::LoadG,
            Channel::B => Instruction::LoadB,
            Channel::A => Instruction::LoadA,
            Channel::T => Instruction::LoadT,
            Channel::L => Instruction::LoadL,
        }]);
    }

    out.push(match target {
        Channel::R => Instruction::LoadR,
        Channel::G => Instruction::LoadG,
        Channel::B => Instruction::LoadB,
        Channel::A => Instruction::LoadA,
        Channel::T => Instruction::LoadT,
        Channel::L => Instruction::LoadL,
    });
    let local_index = local_scope.len() - param_count;
    out.push(Instruction::StoreLocal(local_index));
    local_scope.push(cur_var.to_string());

    compile_stmts_for_channel(
        body,
        &target,
        params,
        param_count,
        &mut local_scope,
        &mut out,
        context,
        filter_decls,
        afilter_decls,
        active_vars,
    )?;

    if let Some(pos) = local_scope[param_count..].iter().position(|p| p == cur_var) {
        out.push(Instruction::LoadLocal(pos));
    }
    Ok(out)
}

fn compile_into(
    expr: &Expr,
    params: &[String],
    param_count: usize,
    out: &mut Vec<Instruction>,
    context: &CompileContext,
    filter_decls: &HashMap<String, FilterDecl>,
    afilter_decls: &HashMap<String, AudioFilterDecl>,
    active_vars: &HashMap<String, Value>,
) -> Result<(), EngineError> {
    match expr {
        Expr::Int(v) => out.push(Instruction::PushInt(*v)),
        Expr::Float(v) => out.push(Instruction::PushFloat(*v as f32)),

        Expr::Ident(name) => match context {
            CompileContext::Image => match name.as_str() {
                "r" => {
                    if let Some(pos) = params[param_count..].iter().position(|p| p == "__cur_r") {
                        out.push(Instruction::LoadLocal(pos));
                    } else {
                        out.push(Instruction::LoadR);
                    }
                }
                "g" => {
                    if let Some(pos) = params[param_count..].iter().position(|p| p == "__cur_g") {
                        out.push(Instruction::LoadLocal(pos));
                    } else {
                        out.push(Instruction::LoadG);
                    }
                }
                "b" => {
                    if let Some(pos) = params[param_count..].iter().position(|p| p == "__cur_b") {
                        out.push(Instruction::LoadLocal(pos));
                    } else {
                        out.push(Instruction::LoadB);
                    }
                }
                "a" => {
                    if let Some(pos) = params[param_count..].iter().position(|p| p == "__cur_a") {
                        out.push(Instruction::LoadLocal(pos));
                    } else {
                        out.push(Instruction::LoadA);
                    }
                }

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
                    } else if let Some(Value::Number(n)) = active_vars.get(other) {
                        // Feature 11: loop variable or outer constant captured into filter
                        out.push(Instruction::PushFloat(*n as f32));
                    } else {
                        return Err(EngineError::Compile(format!(
                            "unknown identifier '{other}' in image filter"
                        )));
                    }
                }
            },

            CompileContext::Audio => match name.as_str() {
                "l" => {
                    if let Some(pos) = params[param_count..].iter().position(|p| p == "__cur_l") {
                        out.push(Instruction::LoadLocal(pos));
                    } else {
                        out.push(Instruction::LoadL);
                    }
                }
                "r" => {
                    if let Some(pos) = params[param_count..].iter().position(|p| p == "__cur_r") {
                        out.push(Instruction::LoadLocal(pos));
                    } else {
                        out.push(Instruction::LoadR);
                    }
                }

                "time" => out.push(Instruction::LoadTime),
                "sr" => out.push(Instruction::LoadSampleRate),

                other => {
                    if let Some(idx) = params.iter().position(|p| p == other) {
                        if idx < param_count {
                            out.push(Instruction::LoadParam(idx));
                        } else {
                            out.push(Instruction::LoadLocal(idx - param_count));
                        }
                    } else if let Some(Value::Number(n)) = active_vars.get(other) {
                        // Feature 11: loop variable or outer constant captured into audio filter
                        out.push(Instruction::PushFloat(*n as f32));
                    } else {
                        return Err(EngineError::Compile(format!(
                            "unknown identifier '{other}' in audio filter"
                        )));
                    }
                }
            },

            CompileContext::Effect => match name.as_str() {
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
                    } else if let Some(Value::Number(n)) = active_vars.get(other) {
                        out.push(Instruction::PushFloat(*n as f32));
                    } else {
                        return Err(EngineError::Compile(format!(
                            "unknown identifier '{other}' in effect"
                        )));
                    }
                }
            },
        },

        Expr::Neg(inner) => {
            compile_into(
                inner,
                params,
                param_count,
                out,
                context,
                filter_decls,
                afilter_decls,
                active_vars,
            )?;
            out.push(Instruction::Neg);
        }
        Expr::Not(inner) => {
            compile_into(
                inner,
                params,
                param_count,
                out,
                context,
                filter_decls,
                afilter_decls,
                active_vars,
            )?;
            out.push(Instruction::Not);
        }

        Expr::BinOp { op, lhs, rhs } => {
            compile_into(
                lhs,
                params,
                param_count,
                out,
                context,
                filter_decls,
                afilter_decls,
                active_vars,
            )?;
            compile_into(
                rhs,
                params,
                param_count,
                out,
                context,
                filter_decls,
                afilter_decls,
                active_vars,
            )?;
            out.push(match op {
                BinOp::Add => Instruction::Add,
                BinOp::Sub => Instruction::Sub,
                BinOp::Mul => Instruction::Mul,
                BinOp::Div => Instruction::Div,
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

            // multi-arg builtins (Features 2 & 8)
            match (name.as_str(), args.len()) {
                ("clamp", 3) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[2],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::Clamp);
                    return Ok(());
                }
                ("lerp", 3) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[2],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::Lerp);
                    return Ok(());
                }
                ("smooth_lerp", 3) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[2],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::SmoothLerp);
                    return Ok(());
                }

                ("min", 2) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::Min);
                    return Ok(());
                }
                ("max", 2) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::Max);
                    return Ok(());
                }
                ("pow", 2) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::Pow);
                    return Ok(());
                }
                ("mod", 2) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::Mod);
                    return Ok(());
                }
                ("atan2", 2) => {
                    compile_into(
                        &args[0],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    compile_into(
                        &args[1],
                        params,
                        param_count,
                        out,
                        context,
                        filter_decls,
                        afilter_decls,
                        active_vars,
                    )?;
                    out.push(Instruction::Atan2);
                    return Ok(());
                }
                _ => {}
            }

            // single-arg math functions (Features 2 & 8)
            if args.len() != 1 {
                return Err(EngineError::Compile(format!(
                    "unsupported call '{name}' with {} args",
                    args.len()
                )));
            }
            compile_into(
                &args[0],
                params,
                param_count,
                out,
                context,
                filter_decls,
                afilter_decls,
                active_vars,
            )?;
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

pub fn compile_audiofilter_decl(
    decl: &AudioFilterDecl,
    filter_decls: &HashMap<String, FilterDecl>,
    afilter_decls: &HashMap<String, AudioFilterDecl>,
    active_vars: &HashMap<String, Value>,
) -> Result<AudioFilter, EngineError> {
    let param_count = decl.params.len();
    Ok(AudioFilter {
        name: decl.name.clone(),
        params: decl.params.clone(),
        l_program: compile_channel_program(
            &decl.body,
            Channel::L,
            &decl.params,
            param_count,
            &CompileContext::Audio,
            filter_decls,
            afilter_decls,
            active_vars,
        )?,
        r_program: compile_channel_program(
            &decl.body,
            Channel::R,
            &decl.params,
            param_count,
            &CompileContext::Audio,
            filter_decls,
            afilter_decls,
            active_vars,
        )?,
    })
}

pub fn compile_filter_decl(
    decl: &FilterDecl,
    filter_decls: &HashMap<String, FilterDecl>,
    afilter_decls: &HashMap<String, AudioFilterDecl>,
    active_vars: &HashMap<String, Value>,
) -> Result<Filter, EngineError> {
    let param_count = decl.params.len();
    Ok(Filter {
        name: decl.name.clone(),
        params: decl.params.clone(),
        r_program: compile_channel_program(
            &decl.body,
            Channel::R,
            &decl.params,
            param_count,
            &CompileContext::Image,
            filter_decls,
            afilter_decls,
            active_vars,
        )?,
        g_program: compile_channel_program(
            &decl.body,
            Channel::G,
            &decl.params,
            param_count,
            &CompileContext::Image,
            filter_decls,
            afilter_decls,
            active_vars,
        )?,
        b_program: compile_channel_program(
            &decl.body,
            Channel::B,
            &decl.params,
            param_count,
            &CompileContext::Image,
            filter_decls,
            afilter_decls,
            active_vars,
        )?,
        a_program: compile_channel_program(
            &decl.body,
            Channel::A,
            &decl.params,
            param_count,
            &CompileContext::Image,
            filter_decls,
            afilter_decls,
            active_vars,
        )?,
    })
}

pub fn compile_kernel(name: &str, matrix_expr: &Expr) -> Result<Kernel, EngineError> {
    let rows = match matrix_expr {
        Expr::Array(r) => r,
        _ => return Err(EngineError::Compile("Kernel must be a 2D matrix literal [[...], [...]]".into())),
    };

    let size = rows.len();
    if size % 2 == 0 {
        return Err(EngineError::Compile("Kernel dimensions must be odd (3x3, 5x5, etc.)".into()));
    }

    let mut weights = Vec::with_capacity(size * size);
    for row_expr in rows {
        let cols = match row_expr {
            Expr::Array(c) => c,
            _ => return Err(EngineError::Compile("Kernel rows must be arrays [...]".into())),
        };

        if cols.len() != size {
            return Err(EngineError::Compile("Kernel must be square (width == height)".into()));
        }

        for cell in cols {
            let val = match cell {
                Expr::Int(i) => *i as f32,
                Expr::Float(f) => *f as f32,
                Expr::Neg(inner) => match &**inner {
                    Expr::Int(i) => -(*i as f32),
                    Expr::Float(f) => -(*f as f32),
                    _ => return Err(EngineError::Compile("Invalid negative cell in kernel".into())),
                },
                _ => return Err(EngineError::Compile("Kernel cells must be numeric literals".into())),
            };
            weights.push(val);
        }
    }

    let sum: f32 = weights.iter().sum();
    let divisor = if sum == 0.0 { 1.0 } else { sum };

    Ok(Kernel {
        name: name.to_string(),
        matrix: weights,
        size,
        divisor,
    })
}

pub struct Engine {
    pub vars: HashMap<String, Value>,
    pub filters: HashMap<String, Filter>,
    pub filter_decls: HashMap<String, FilterDecl>,
    pub afilters: HashMap<String, AudioFilter>,
    pub afilter_decls: HashMap<String, AudioFilterDecl>,
    pub kernels: HashMap<String, Kernel>,
    pub functions: HashMap<String, FnDecl>,
    pub imported_files: HashSet<String>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            filters: HashMap::new(),
            filter_decls: HashMap::new(),
            afilters: HashMap::new(),
            afilter_decls: HashMap::new(),
            kernels: HashMap::new(),
            functions: HashMap::new(),
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
                        let mut std = match find_stdlib() {
                            Some(value) => value,
                            None => {
                                return Err(EngineError::Eval(
                                    "Could not locate the DriveLang standard library. Place a complete stdlib/ folder beside drive or in the project, or set DRIVELANG_STDLIB to its location.".into(),
                                ));
                            }
                        };
                        for component in path.iter().skip(1) {
                            std.push(component);
                        }

                        std.set_extension("drive");

                        self.import_file(std.to_str().unwrap())?;
                    }
                }
                Ok(())
            }

            Item::FnDecl(decl) => {
                self.functions.insert(decl.name.clone(), decl.clone());
                Ok(())
            }

            Item::AudioFilterDecl(decl) => {
                self.afilter_decls.insert(decl.name.clone(), decl.clone());
                let afilter = compile_audiofilter_decl(
                    decl,
                    &self.filter_decls,
                    &self.afilter_decls,
                    &self.vars,
                )?;
                self.afilters.insert(decl.name.clone(), afilter);
                Ok(())
            }

            Item::FilterDecl(decl) => {
                self.filter_decls.insert(decl.name.clone(), decl.clone());
                let filter = compile_filter_decl(
                    decl,
                    &self.filter_decls,
                    &self.afilter_decls,
                    &self.vars,
                )?;
                self.filters.insert(decl.name.clone(), filter);
                Ok(())
            }

            Item::KernelDecl { name, matrix } => {
                let kernel = compile_kernel(name, matrix)?;
                self.kernels.insert(name.clone(), kernel);
                Ok(())
            }

            Item::Assign { name, value } => {
                let val = self.eval(value)?;
                self.vars.insert(name.clone(), val);
                Ok(())
            }

            Item::ForLoop {
                variable,
                iterable,
                items,
            } => {
                match iterable {
                    ForIterable::Range(range_expr) => {
                        let step_range = self.expr_to_step_range(range_expr)?;
                        for val in step_range.iter() {
                            self.vars
                                .insert(variable.clone(), Value::Number(val as f64));
                            for inner_item in items {
                                self.exec_item(inner_item)?;
                            }
                        }
                    }
                    ForIterable::Folder(folder_name) => {
                        let folder = match self.vars.get(folder_name) {
                            Some(Value::Folder(f)) => f.clone(),
                            _ => {
                                return Err(EngineError::UndefinedVar(format!(
                                    "Folder '{folder_name}' not found"
                                )));
                            }
                        };
                        for entry in folder.entries() {
                            let mut frame =
                                io::load_image(entry.path.to_str().unwrap_or(""), "rgba")
                                    .map_err(|e| EngineError::Eval(format!("{e}")))?;
                            frame.set_source_name(entry.name.clone());
                            self.vars.insert(variable.clone(), Value::Frame(frame));
                            for inner_item in items {
                                self.exec_item(inner_item)?;
                            }
                        }
                    }
                }
                Ok(())
            }

            Item::IfElse {
                cond,
                true_branch,
                false_branch,
            } => {
                let cond_val = self.eval(cond)?;
                let is_true = match cond_val {
                    Value::Number(n) => n != 0.0,
                    _ => {
                        return Err(EngineError::EvalError(
                            "If condition must be a number".into(),
                        ));
                    }
                };

                let branch_to_run = if is_true { true_branch } else { false_branch };
                for inner_item in branch_to_run {
                    self.exec_item(inner_item)?;
                }
                Ok(())
            }

            Item::Print { args } => {
                if args.is_empty() {
                    println!();
                    return Ok(());
                }
                let first_val = self.eval(&args[0])?;
                let mut fmt_str = match first_val {
                    Value::String(s) => s,
                    other => {
                        let s = match other {
                            Value::Number(n) => {
                                if n.fract() == 0.0 && n.abs() < 1e15 {
                                    (n as i64).to_string()
                                } else {
                                    n.to_string()
                                }
                            }
                            Value::Frame(f) => format!("<Frame {}x{}>", f.width(), f.height()),
                            Value::Track(t) => {
                                format!("<Track {}Hz {}ch>", t.sample_rate(), t.channels())
                            }
                            Value::Folder(f) => {
                                format!("<Folder '{}' ({} images)>", f.dir.display(), f.len())
                            }
                            _ => "".to_string(),
                        };
                        println!("{s}");
                        return Ok(());
                    }
                };

                let mut arg_index = 1;
                while let Some(placeholder_pos) = fmt_str.find("{}") {
                    if arg_index >= args.len() {
                        return Err(EngineError::Eval(
                            "Not enough arguments provided for format string!".to_string(),
                        ));
                    }
                    let val = self.eval(&args[arg_index])?;
                    let val_str = match val {
                        Value::Number(n) => {
                            if n.fract() == 0.0 && n.abs() < 1e15 {
                                (n as i64).to_string()
                            } else {
                                n.to_string()
                            }
                        }
                        Value::String(s) => s,
                        Value::Frame(f) => format!("<Frame {}x{}>", f.width(), f.height()),
                        Value::Track(t) => {
                            format!("<Track {}Hz {}ch>", t.sample_rate(), t.channels())
                        }
                        Value::Folder(f) => {
                            format!("<Folder '{}' ({} images)>", f.dir.display(), f.len())
                        }
                    };
                    fmt_str.replace_range(placeholder_pos..placeholder_pos + 2, &val_str);
                    arg_index += 1;
                }
                println!("{fmt_str}");
                Ok(())
            }

            Item::Export { value, path } => {
                let path_val = self.eval(path)?;
                let path_str = match path_val {
                    Value::String(s) => s,
                    _ => {
                        return Err(EngineError::Eval(
                            "export path must evaluate to a string".into(),
                        ));
                    }
                };
                let export_val = self.eval(value)?;
                match export_val {
                    Value::Frame(f) => {
                        io::encode_image(&f, &path_str).map_err(|_| {
                            EngineError::Eval(format!(
                                "Image Export Failed to '{path_str}'! Either Frame is Empty or invalid"
                            ))
                        })?;
                    }
                    Value::Track(t) => {
                        let path = Path::new(&path_str);
                        if path_str.ends_with(".mp3") {
                            // Feature 9: Native MP3 Export
                            io::encode_mp3(&t, path, None).map_err(|e| {
                                EngineError::Eval(format!("MP3 Encoding failed: {e}"))
                            })?;
                        } else {
                            io::encode_wav(&t, path)
                                .map_err(|_| EngineError::Eval("Encoding Audio failed! Invalid Track".into()))?;
                        }
                    }
                    _ => {
                        return Err(EngineError::Eval(
                            "export requires a Frame or Track".into(),
                        ));
                    }
                }
                Ok(())
            }

            Item::Expr(expr) => {
                self.eval(expr)?;
                Ok(())
            }
        }
    }

    // ── Function execution ──────────────────────────────────────────────────
    pub fn exec_fn(&mut self, decl: &FnDecl, arg_vals: &[Value]) -> Result<Value, EngineError> {
        let mut fn_env = self.vars.clone();
        for (i, param) in decl.params.iter().enumerate() {
            if let Some(val) = arg_vals.get(i) {
                fn_env.insert(param.clone(), val.clone());
            }
        }
        self.exec_fn_body(&decl.body, &mut fn_env)
    }

    fn exec_fn_body(
        &mut self,
        stmts: &[Statement],
        env: &mut HashMap<String, Value>,
    ) -> Result<Value, EngineError> {
        for stmt in stmts {
            match stmt {
                Statement::Return(expr) => {
                    return self.eval_with_env(expr, env);
                }
                Statement::Let { name, value } => {
                    let val = self.eval_with_env(value, env)?;
                    env.insert(name.clone(), val);
                }
                Statement::IfElse {
                    cond,
                    true_branch,
                    false_branch,
                } => {
                    let cond_val = self.eval_with_env(cond, env)?;
                    let is_true = match cond_val {
                        Value::Number(n) => n != 0.0,
                        _ => false,
                    };
                    let branch = if is_true { true_branch } else { false_branch };
                    let res = self.exec_fn_body(branch, env)?;
                    if !matches!(res, Value::Number(0.0)) || branch.iter().any(|s| matches!(s, Statement::Return(_))) {
                        return Ok(res);
                    }
                }
                Statement::ForLoop {
                    variable,
                    start,
                    end,
                    step,
                    body,
                } => {
                    let s = match self.eval_with_env(start, env)? {
                        Value::Number(n) => n,
                        _ => 0.0,
                    };
                    let e = match self.eval_with_env(end, env)? {
                        Value::Number(n) => n,
                        _ => 0.0,
                    };
                    let st = match step {
                        Some(step_expr) => match self.eval_with_env(step_expr, env)? {
                            Value::Number(n) => n.max(1.0),
                            _ => 1.0,
                        },
                        None => 1.0,
                    };
                    let mut i = s;
                    while i < e {
                        env.insert(variable.clone(), Value::Number(i));
                        let res = self.exec_fn_body(body, env)?;
                        if body.iter().any(|s| matches!(s, Statement::Return(_))) {
                            return Ok(res);
                        }
                        i += st;
                    }
                }
                Statement::Channel(ChannelAssign { value, .. }) => {
                    let _ = self.eval_with_env(value, env)?;
                }
                Statement::FilterCall { name, args } => {
                    let mut arg_vals = Vec::new();
                    for a in args {
                        arg_vals.push(self.eval_with_env(a, env)?);
                    }
                    if let Some(f) = self.functions.get(name).cloned() {
                        let _ = self.exec_fn(&f, &arg_vals)?;
                    }
                }
            }
        }
        Ok(Value::Number(0.0))
    }

    fn eval_with_env(
        &mut self,
        expr: &Expr,
        env: &mut HashMap<String, Value>,
    ) -> Result<Value, EngineError> {
        let prev_vars = std::mem::replace(&mut self.vars, env.clone());
        let res = self.eval(expr);
        *env = self.vars.clone();
        self.vars = prev_vars;
        res
    }

    // ── Evaluation ──────────────────────────────────────────────────────────

    pub fn eval(&mut self, expr: &Expr) -> Result<Value, EngineError> {
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
                _ => Err(EngineError::Eval("cannot negate non-numeric value".into())),
            },
            Expr::Not(inner) => match self.eval(inner)? {
                Value::Number(n) => Ok(Value::Number(if n == 0.0 { 1.0 } else { 0.0 })),
                _ => Err(EngineError::Eval("cannot apply 'not' to non-numeric value".into())),
            },
            Expr::Str(s) => Ok(Value::String(s.clone())),

            // Feature 6: Formatted strings (F-strings)
            Expr::FString(segments) => {
                let mut out = String::new();
                for seg in segments {
                    match seg {
                        FStringSegment::Literal(s) => out.push_str(s),
                        FStringSegment::Interpolated { expr, spec } => {
                            let val = self.eval(expr)?;
                            match val {
                                Value::Number(n) => {
                                    if let Some(spec) = spec {
                                        if spec.starts_with('0') && spec.len() > 1 {
                                            if let Ok(width) = spec[1..].parse::<usize>() {
                                                out.push_str(&format!("{:0width$}", n as i64, width = width));
                                            } else {
                                                out.push_str(&n.to_string());
                                            }
                                        } else if spec.starts_with('.') && spec.ends_with('f') {
                                            if let Ok(prec) = spec[1..spec.len() - 1].parse::<usize>() {
                                                out.push_str(&format!("{:.prec$}", n, prec = prec));
                                            } else {
                                                out.push_str(&n.to_string());
                                            }
                                        } else {
                                            out.push_str(&n.to_string());
                                        }
                                    } else if n.fract() == 0.0 && n.abs() < 1e15 {
                                        out.push_str(&(n as i64).to_string());
                                    } else {
                                        out.push_str(&n.to_string());
                                    }
                                }
                                Value::String(s) => out.push_str(&s),
                                Value::Frame(f) => {
                                    out.push_str(&format!("<Frame {}x{}>", f.width(), f.height()));
                                }
                                Value::Track(t) => {
                                    out.push_str(&format!("<Track {}Hz {}ch>", t.sample_rate(), t.channels()));
                                }
                                Value::Folder(f) => {
                                    out.push_str(&format!("<Folder '{}' ({} images)>", f.dir.display(), f.len()));
                                }
                            }
                        }
                    }
                }
                Ok(Value::String(out))
            }

            // Feature 3: Indexing folder[key]
            Expr::Index { base, key } => {
                let base_val = self.eval(base)?;
                match base_val {
                    Value::Folder(folder) => {
                        let key_val = self.eval(key)?;
                        match key_val {
                            Value::String(name) => {
                                if let Some(entry) = folder.get(&name) {
                                    let mut frame = io::load_image(entry.path.to_str().unwrap_or(""), "rgba")
                                        .map_err(|e| EngineError::Eval(format!("{e}")))?;
                                    frame.set_source_name(entry.name.clone());
                                    Ok(Value::Frame(frame))
                                } else {
                                    Err(EngineError::Eval(format!(
                                        "File '{name}' not found in folder '{}'",
                                        folder.dir.display()
                                    )))
                                }
                            }
                            Value::Number(idx) => {
                                let i = idx as usize;
                                if let Some(entry) = folder.entries().get(i) {
                                    let mut frame = io::load_image(entry.path.to_str().unwrap_or(""), "rgba")
                                        .map_err(|e| EngineError::Eval(format!("{e}")))?;
                                    frame.set_source_name(entry.name.clone());
                                    Ok(Value::Frame(frame))
                                } else {
                                    Err(EngineError::Eval(format!(
                                        "Index {i} out of bounds for folder with {} files",
                                        folder.len()
                                    )))
                                }
                            }
                            _ => Err(EngineError::Eval(
                                "Folder index must be a filename string or integer".into(),
                            )),
                        }
                    }
                    _ => Err(EngineError::Eval(
                        "indexing is only supported on Folder values".into(),
                    )),
                }
            }

            // Feature 4: Field access (img.width, img.height, img.name, folder.len)
            Expr::FieldAccess { base, field } => {
                let val = self.eval(base)?;
                match val {
                    Value::Frame(f) => match field.as_str() {
                        "width" => Ok(Value::Number(f.width() as f64)),
                        "height" => Ok(Value::Number(f.height() as f64)),
                        "name" => Ok(Value::String(f.source_name().unwrap_or("").to_string())),
                        other => Err(EngineError::Eval(format!("Unknown field '{other}' on Frame"))),
                    },
                    Value::Folder(folder) => match field.as_str() {
                        "len" => Ok(Value::Number(folder.len() as f64)),
                        "path" => Ok(Value::String(folder.dir.to_str().unwrap_or("").to_string())),
                        other => Err(EngineError::Eval(format!("Unknown field '{other}' on Folder"))),
                    },
                    _ => Err(EngineError::Eval(format!(
                        "Field access '.{field}' not supported on this type"
                    ))),
                }
            }

            // Method call (photos.len(), img.width())
            Expr::MethodCall { base, method, .. } => {
                let val = self.eval(base)?;
                match val {
                    Value::Folder(folder) => match method.as_str() {
                        "len" => Ok(Value::Number(folder.len() as f64)),
                        other => Err(EngineError::Eval(format!("Unknown method '{other}' on Folder"))),
                    },
                    Value::Frame(f) => match method.as_str() {
                        "width" => Ok(Value::Number(f.width() as f64)),
                        "height" => Ok(Value::Number(f.height() as f64)),
                        "name" => Ok(Value::String(f.source_name().unwrap_or("").to_string())),
                        other => Err(EngineError::Eval(format!("Unknown method '{other}' on Frame"))),
                    },
                    _ => Err(EngineError::Eval(format!(
                        "Method call '.{method}()' not supported on this type"
                    ))),
                }
            }

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
                    BinOp::Eq => if l == r { 1.0 } else { 0.0 },
                    BinOp::Ne => if l != r { 1.0 } else { 0.0 },
                    BinOp::Gt => if l > r { 1.0 } else { 0.0 },
                    BinOp::Ge => if l >= r { 1.0 } else { 0.0 },
                    BinOp::Lt => if l < r { 1.0 } else { 0.0 },
                    BinOp::Le => if l <= r { 1.0 } else { 0.0 },
                    BinOp::And => if l != 0.0 && r != 0.0 { 1.0 } else { 0.0 },
                    BinOp::Or => if l != 0.0 || r != 0.0 { 1.0 } else { 0.0 },
                }))
            }

            Expr::Call { path, args } => self.eval_call(path, args),

            Expr::Pipe { base, stages } => match self.eval(base)? {
                Value::Frame(mut frame) => {
                    let mut pipeline = EffectPipeline {
                        operations: Vec::new(),
                    };
                    for stage in stages {
                        pipeline.operations.push(self.compile_stage(stage)?);
                    }
                    pipeline.execute(&mut frame)?;
                    Ok(Value::Frame(frame))
                }
                Value::Track(mut track) => {
                    let mut pipeline = AudioPipeline {
                        operations: Vec::new(),
                    };
                    for stage in stages {
                        pipeline.operations.push(self.compile_audio(stage)?);
                    }
                    pipeline.execute(&mut track)?;
                    Ok(Value::Track(track))
                }
                _ => Err(EngineError::Eval(
                    "Piping '->' is only supported on Frames and Tracks!".into(),
                )),
            },

            other => Err(EngineError::Eval(format!(
                "cannot evaluate expression: {other:?}"
            ))),
        }
    }

    fn eval_call(&mut self, path: &[String], args: &[Expr]) -> Result<Value, EngineError> {
        let name = path.last().map(String::as_str).unwrap_or("");
        match name {
            // Media constructors
            "frame" => {
                let path_str = match args.first() {
                    Some(expr) => self.eval_string(expr)?,
                    None => return Err(EngineError::Eval("frame() requires a string path".into())),
                };

                let mut frame = io::load_image(&path_str, "rgba")
                    .map_err(|e| EngineError::Eval(format!("{e}")))?;
                if let Some(file_name) = Path::new(&path_str).file_name().and_then(|n| n.to_str()) {
                    frame.set_source_name(file_name.to_string());
                }
                Ok(Value::Frame(frame))
            }

            "track" => {
                let path_str = match args.first() {
                    Some(expr) => self.eval_string(expr)?,
                    None => return Err(EngineError::Eval("track() requires a string path".into())),
                };
                let path = Path::new(&path_str);
                let track = io::decode_audio(path).map_err(|_| {
                    EngineError::Eval("Audio Decoding Failed, Check Path....".into())
                })?;
                Ok(Value::Track(track))
            }

            // Feature 3: folder("path")
            "folder" => {
                let path_str = match args.first() {
                    Some(expr) => self.eval_string(expr)?,
                    None => return Err(EngineError::Eval("folder() requires a string directory path".into())),
                };
                let folder = Folder::open(&path_str).map_err(|e| EngineError::Eval(format!("{e}")))?;
                Ok(Value::Folder(folder))
            }

            "len" => {
                if args.is_empty() {
                    return Err(EngineError::Eval("len() requires 1 argument".into()));
                }
                match self.eval(&args[0])? {
                    Value::Folder(f) => Ok(Value::Number(f.len() as f64)),
                    Value::String(s) => Ok(Value::Number(s.len() as f64)),
                    _ => Err(EngineError::Eval("len() only supported on Folder or String".into())),
                }
            }

            "text" => {
                if args.len() < 6 {
                    return Err(EngineError::EvalError(
                        "text() needs 6 args: str, font_path, size, r, g, b".into(),
                    ));
                }

                let text_content = self.eval_string(&args[0])?;
                let font_path = self.eval_string(&args[1])?;

                let size = self.eval_number(&args[2])? as f32;
                let r = self.eval_number(&args[3])? as u8;
                let g = self.eval_number(&args[4])? as u8;
                let b = self.eval_number(&args[5])? as u8;
                let bytes = match std::fs::read(&font_path) {
                    Ok(b) => b,
                    Err(_) => return Err(EngineError::Eval("Invalid Font Path".into())),
                };
                let font = Font::from_bytes(bytes, FontSettings::default())
                    .expect("Font Loading Failed, check path");

                let mut txt_obj = Text::new(
                    &text_content,
                    font,
                    size,
                    Pos(0, 0),
                    Color::RGBA(r, g, b, 255),
                );
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
                        "silence() requires Duration, sample rate and channel".into(),
                    ));
                }

                let dur = self.eval_number(&args[0])?;
                let duration = TimeStamp::from_seconds(dur, 1, 44100);
                let sample_rate = self.eval_number(&args[1])? as u32;
                let channel = self.eval_number(&args[2])? as u16;

                Ok(Value::Track(Track::silence(duration, sample_rate, channel)))
            }

            // Feature 10: Rich audio functions
            "merge" => {
                if args.len() != 2 {
                    return Err(EngineError::Eval("merge() requires exactly 2 tracks".into()));
                }
                let a = self.eval_track(&args[0])?;
                let b = self.eval_track(&args[1])?;
                let mixed = Track::mix_sum(&a, &b).map_err(|e| EngineError::Eval(format!("{e}")))?;
                Ok(Value::Track(mixed))
            }

            "concat" => {
                if args.is_empty() {
                    return Err(EngineError::Eval("concat() requires at least 1 track".into()));
                }
                let mut tracks = Vec::new();
                for arg in args {
                    tracks.push(self.eval_track(arg)?);
                }
                let track_refs: Vec<&Track> = tracks.iter().collect();
                let concatenated =
                    Track::concat_tracks(&track_refs).map_err(|e| EngineError::Eval(format!("{e}")))?;
                Ok(Value::Track(concatenated))
            }

            "slice" => {
                if args.len() != 3 {
                    return Err(EngineError::Eval(
                        "slice() requires 3 arguments: track, start_seconds, end_seconds".into(),
                    ));
                }
                let track = self.eval_track(&args[0])?;
                let start = self.eval_number(&args[1])?;
                let end = self.eval_number(&args[2])?;
                Ok(Value::Track(track.slice_seconds(start, end)))
            }

            "channel_split" => {
                if args.len() != 2 {
                    return Err(EngineError::Eval(
                        "channel_split() requires 2 arguments: track, channel_index".into(),
                    ));
                }
                let track = self.eval_track(&args[0])?;
                let ch = self.eval_usize(&args[1])?;
                Ok(Value::Track(track.channel_split(ch)))
            }

            "reverse" => {
                if args.len() != 1 {
                    return Err(EngineError::Eval("reverse() requires 1 track argument".into()));
                }
                let track = self.eval_track(&args[0])?;
                Ok(Value::Track(track.reverse()))
            }

            "speed" => {
                if args.len() != 2 {
                    return Err(EngineError::Eval("speed() requires 2 arguments: track, factor".into()));
                }
                let track = self.eval_track(&args[0])?;
                let factor = self.eval_number(&args[1])?;
                Ok(Value::Track(track.speed(factor)))
            }

            "export_mp3" => {
                // Feature 9: Explicit MP3 export call
                if args.len() < 2 {
                    return Err(EngineError::Eval("export_mp3() requires track and path".into()));
                }
                let track = self.eval_track(&args[0])?;
                let path = self.eval_string(&args[1])?;
                let kbps = if args.len() >= 3 {
                    Some(self.eval_number(&args[2])? as u32)
                } else {
                    None
                };
                io::encode_mp3(&track, Path::new(&path), kbps)
                    .map_err(|e| EngineError::Eval(format!("{e}")))?;
                Ok(Value::Track(track))
            }

            // Features 2 & 8: Complete Mathematical Built-ins
            "abs" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.abs()))
            }
            "sin" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.sin()))
            }
            "cos" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.cos()))
            }
            "tan" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.tan()))
            }
            "asin" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.asin()))
            }
            "acos" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.acos()))
            }
            "atan" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.atan()))
            }
            "atan2" => {
                let y = self.eval_number(&args[0])?;
                let x = self.eval_number(&args[1])?;
                Ok(Value::Number(y.atan2(x)))
            }
            "sqrt" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.sqrt()))
            }
            "exp" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.exp()))
            }
            "log" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.ln()))
            }
            "log10" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.log10()))
            }
            "floor" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.floor()))
            }
            "ceil" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.ceil()))
            }
            "round" => {
                let x = self.eval_number(&args[0])?;
                Ok(Value::Number(x.round()))
            }
            "min" => {
                let a = self.eval_number(&args[0])?;
                let b = self.eval_number(&args[1])?;
                Ok(Value::Number(a.min(b)))
            }
            "max" => {
                let a = self.eval_number(&args[0])?;
                let b = self.eval_number(&args[1])?;
                Ok(Value::Number(a.max(b)))
            }
            "pow" => {
                let a = self.eval_number(&args[0])?;
                let b = self.eval_number(&args[1])?;
                Ok(Value::Number(a.powf(b)))
            }
            "mod" => {
                let a = self.eval_number(&args[0])?;
                let b = self.eval_number(&args[1])?;
                Ok(Value::Number(if b == 0.0 { 0.0 } else { a % b }))
            }
            "clamp" => {
                let x = self.eval_number(&args[0])?;
                let lo = self.eval_number(&args[1])?;
                let hi = self.eval_number(&args[2])?;
                Ok(Value::Number(x.clamp(lo, hi)))
            }
            "lerp" => {
                let a = self.eval_number(&args[0])?;
                let b = self.eval_number(&args[1])?;
                let t = self.eval_number(&args[2])?;
                Ok(Value::Number(a + (b - a) * t))
            }
            "smooth_lerp" => {
                let a = self.eval_number(&args[0])?;
                let b = self.eval_number(&args[1])?;
                let t = self.eval_number(&args[2])?;
                let st = t * t * (3.0 - 2.0 * t);
                Ok(Value::Number(a + (b - a) * st))
            }

            other => {
                // Check if user-defined function exists
                if let Some(decl) = self.functions.get(other).cloned() {
                    let mut arg_vals = Vec::new();
                    for a in args {
                        arg_vals.push(self.eval(a)?);
                    }
                    return self.exec_fn(&decl, &arg_vals);
                }

                Err(EngineError::UndefinedOp(format!("unknown function '{other}'")))
            }
        }
    }

    pub fn eval_frame(&mut self, expr: &Expr) -> Result<Frame, EngineError> {
        match self.eval(expr)? {
            Value::Frame(f) => Ok(f),
            _ => Err(EngineError::Eval("expected a frame".into())),
        }
    }

    pub fn eval_track(&mut self, expr: &Expr) -> Result<Track, EngineError> {
        match self.eval(expr)? {
            Value::Track(t) => Ok(t),
            _ => Err(EngineError::Eval("expected a track".into())),
        }
    }

    pub fn eval_number(&mut self, expr: &Expr) -> Result<f64, EngineError> {
        match self.eval(expr)? {
            Value::Number(n) => Ok(n),
            _ => Err(EngineError::Eval("expected a number".into())),
        }
    }

    pub fn eval_string(&mut self, expr: &Expr) -> Result<String, EngineError> {
        match self.eval(expr)? {
            Value::String(s) => Ok(s),
            _ => Err(EngineError::Eval("expected a string".into())),
        }
    }

    pub fn eval_usize(&mut self, expr: &Expr) -> Result<usize, EngineError> {
        let n = self.eval_number(expr)?;
        if n < 0.0 {
            return Err(EngineError::Eval("bound must be non-negative".into()));
        }
        Ok(n as usize)
    }

    fn compile_audio(
        &mut self,
        stage: &PipeStage,
    ) -> Result<AudioOperation, EngineError> {
        let name = stage
            .path
            .last()
            .ok_or_else(|| EngineError::Eval("Empty pipeline stage".into()))?;

        // Feature 10: Rich audio operations in pipelines
        match name.as_str() {
            "reverse" => return Ok(AudioOperation::Reverse),
            "speed" => {
                if stage.args.is_empty() {
                    return Err(EngineError::Eval("speed() requires a numeric factor".into()));
                }
                let factor = self.eval_number(&stage.args[0])?;
                return Ok(AudioOperation::Speed(factor));
            }
            "fade_in" => {
                if stage.args.is_empty() {
                    return Err(EngineError::Eval("fade_in() requires duration in seconds".into()));
                }
                let dur = self.eval_number(&stage.args[0])?;
                return Ok(AudioOperation::FadeIn(dur));
            }
            "fade_out" => {
                if stage.args.is_empty() {
                    return Err(EngineError::Eval("fade_out() requires duration in seconds".into()));
                }
                let dur = self.eval_number(&stage.args[0])?;
                return Ok(AudioOperation::FadeOut(dur));
            }
            "gain" => {
                if stage.args.is_empty() {
                    return Err(EngineError::Eval("gain() requires dB number".into()));
                }
                let db = self.eval_number(&stage.args[0])? as f32;
                return Ok(AudioOperation::Gain(db));
            }
            _ => {}
        }

        let mut params = Vec::new();
        for arg in &stage.args {
            match self.eval(arg)? {
                Value::Number(v) => params.push(v as f32),
                _ => {
                    return Err(EngineError::Eval(format!(
                        "Audio filter '{}' only accepts numeric parameters.",
                        name
                    )));
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
        stage: &PipeStage,
    ) -> Result<Operation, EngineError> {
        let name = stage
            .path
            .last()
            .ok_or_else(|| EngineError::Compile("empty stage path".into()))?;

        if name.as_str() == "resize" {
            if stage.args.len() != 2 {
                return Err(EngineError::Compile(
                    "resize requires exactly 2 arguments: (width, height)".into(),
                ));
            }
            let width = self.eval_number(&stage.args[0])?.max(1.0) as u32;
            let height = self.eval_number(&stage.args[1])?.max(1.0) as u32;
            return Ok(Operation::NativeResize { width, height });
        }

        if name.as_str() == "crop" {
            if stage.args.len() != 4 {
                return Err(EngineError::Compile(
                    "crop requires exactly 4 arguments: (x, y, width, height)".into(),
                ));
            }
            let x = self.eval_number(&stage.args[0])?.max(0.0) as u32;
            let y = self.eval_number(&stage.args[1])?.max(0.0) as u32;
            let width = self.eval_number(&stage.args[2])?.max(1.0) as u32;
            let height = self.eval_number(&stage.args[3])?.max(1.0) as u32;
            return Ok(Operation::NativeCrop {
                x,
                y,
                width,
                height,
            });
        }

        if name.as_str() == "blend" {
            if stage.args.len() != 4 {
                return Err(EngineError::Compile(
                    "blend requires exactly 4 arguments: (x, y, frame, alpha)".into(),
                ));
            }
            let x = self.eval_number(&stage.args[0])?.max(0.0) as u32;
            let y = self.eval_number(&stage.args[1])?.max(0.0) as u32;
            let frame = self.eval_frame(&stage.args[2]).expect("Not a valid Frame.");
            let alpha = self.eval_number(&stage.args[3])?.max(0.0);
            return Ok(Operation::Blend {
                x,
                y,
                frame2: frame,
                alpha,
            });
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

        if name.as_str() == "blur" && !stage.args.is_empty() {
            let size_f64 = self.eval_number(&stage.args[0])?;
            let size = size_f64.max(1.0) as usize;
            let dynamic_kernel = Kernel::generate_blur("blur", size);
            return Ok(Operation::Convolution {
                kernel: dynamic_kernel,
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
