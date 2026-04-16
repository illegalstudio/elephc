use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "exit" | "die" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "exit() takes 0 or 1 arguments"));
            }
            if let Some(arg) = args.first() {
                let ty = checker.infer_type(arg, env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(span, "exit() argument must be integer"));
                }
            }
            Ok(Some(PhpType::Void))
        }
        "is_bool" | "boolval" | "is_null" | "is_float" | "is_int" | "is_string"
        | "is_numeric" | "is_nan" | "is_finite" | "is_infinite" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "floatval" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "floatval() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Float))
        }
        "abs" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "abs() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            match ty {
                PhpType::Float => Ok(Some(PhpType::Float)),
                _ => Ok(Some(PhpType::Int)),
            }
        }
        "floor" | "ceil" | "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
        | "sinh" | "cosh" | "tanh" | "log2" | "log10" | "exp" | "deg2rad"
        | "rad2deg" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Float))
        }
        "log" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "log() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Float))
        }
        "atan2" | "hypot" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Float))
        }
        "pi" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "pi() takes no arguments"));
            }
            Ok(Some(PhpType::Float))
        }
        "round" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "round() takes 1 or 2 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            if args.len() == 2 {
                checker.infer_type(&args[1], env)?;
            }
            Ok(Some(PhpType::Float))
        }
        "pow" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "pow() takes exactly 2 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Float))
        }
        "min" | "max" => {
            if args.len() < 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() requires at least 2 arguments", name),
                ));
            }
            let mut has_float = false;
            for arg in args {
                let t = checker.infer_type(arg, env)?;
                if t == PhpType::Float {
                    has_float = true;
                }
            }
            if has_float {
                Ok(Some(PhpType::Float))
            } else {
                Ok(Some(PhpType::Int))
            }
        }
        "intdiv" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "intdiv() takes exactly 2 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Int))
        }
        "fmod" | "fdiv" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Float))
        }
        "rand" | "mt_rand" => {
            if !args.is_empty() && args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 0 or 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "random_int" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "random_int() takes exactly 2 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Int))
        }
        "number_format" => {
            if args.is_empty() || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    "number_format() takes 1 to 4 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "gettype" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "gettype() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "empty" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "empty() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "unset" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "unset() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "settype" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "settype() takes exactly 2 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            let ty = checker.infer_type(&args[1], env)?;
            if ty != PhpType::Str {
                return Err(CompileError::new(
                    span,
                    "settype() second argument must be a string",
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        "buffer_len" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "buffer_len() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    span,
                    "buffer_len() argument must be buffer<T>",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "buffer_free" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "buffer_free() takes exactly 1 argument"));
            }
            match &args[0].kind {
                ExprKind::Variable(name) => {
                    if checker.current_class.is_some() && name == "this" {
                        return Err(CompileError::new(span, "buffer_free() cannot free $this"));
                    }
                    if checker.active_ref_params.contains(name)
                        || checker.active_globals.contains(name)
                        || checker.active_statics.contains(name)
                    {
                        return Err(CompileError::new(
                            span,
                            "buffer_free() argument must be a local variable",
                        ));
                    }
                }
                _ => {
                    let ty = checker.infer_type(&args[0], env)?;
                    if !matches!(ty, PhpType::Buffer(_)) {
                        return Err(CompileError::new(
                            span,
                            "buffer_free() argument must be buffer<T>",
                        ));
                    }
                    return Err(CompileError::new(
                        span,
                        "buffer_free() argument must be a local variable",
                    ));
                }
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    span,
                    "buffer_free() argument must be buffer<T>",
                ));
            }
            Ok(Some(PhpType::Void))
        }
        _ => Ok(None),
    }
}
