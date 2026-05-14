//! Purpose:
//! Type-checks SPL helper builtins implemented by the current SPL foundation.
//! Enforces conservative argument contracts that the AOT codegen can lower safely.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Autoload helpers are static/AOT approximations rather than runtime code loaders.
//! - `spl_autoload_extensions()` only accepts literal setters until the runtime owns copied strings.

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
        "spl_autoload_register" => {
            if args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_register() takes at most 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "spl_autoload_unregister" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_unregister() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "spl_autoload_functions" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_functions() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Mixed))))
        }
        "spl_autoload_extensions" => {
            if args.len() > 1 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_extensions() takes at most 1 argument",
                ));
            }
            if let Some(arg) = args.first() {
                checker.infer_type(arg, env)?;
                if !matches!(
                    arg.kind,
                    ExprKind::StringLiteral(_) | ExprKind::Null
                ) {
                    return Err(CompileError::new(
                        span,
                        "spl_autoload_extensions() argument must be a string literal or null",
                    ));
                }
            }
            Ok(Some(PhpType::Str))
        }
        "spl_autoload_call" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_call() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "spl_autoload" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload() takes 1 or 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "spl_object_id" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_object_id() takes exactly 1 argument",
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Object(_)) {
                return Err(CompileError::new(
                    span,
                    "spl_object_id() argument must be an object",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "spl_object_hash" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_object_hash() takes exactly 1 argument",
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Object(_)) {
                return Err(CompileError::new(
                    span,
                    "spl_object_hash() argument must be an object",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "spl_classes" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "spl_classes() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        _ => Ok(None),
    }
}
