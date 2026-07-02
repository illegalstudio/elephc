//! Purpose:
//! Type-checks PHP IO builtin debug helpers and signatures.
//! Validates arity, argument categories, resource handling, and return types before codegen sees calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::io::check_builtin()`
//!
//! Key details:
//! - Return types and diagnostics must stay aligned with `crate::types::signatures` and builtin codegen emitters.
//! - `var_dump` is variadic (1+ args, returns void).
//! - `print_r` accepts an optional `$return` bool; returns `Str` when `$return` is truthy, else `Void`/`True`.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::common::BuiltinResult;
use super::super::super::Checker;

/// Type-checks `var_dump` (variadic, returns void) and `print_r` (optional `$return`).
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "var_dump" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "var_dump() requires at least 1 argument"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "print_r" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "print_r() takes 1 or 2 arguments",
                ));
            }
            checker.infer_type(&args[0], env)?;
            if args.len() == 2 {
                let ret_ty = checker.infer_type(&args[1], env)?;
                // When $return is truthy (literal true or non-zero), print_r returns a string.
                if matches!(ret_ty, PhpType::Bool) {
                if let ExprKind::BoolLiteral(v) = &args[1].kind {
                    if *v {
                        return Ok(Some(PhpType::Str));
                    }
                }
                }
                // Default: returns true (non-return mode echoes and returns true).
                Ok(Some(PhpType::Bool))
            } else {
                Ok(Some(PhpType::Bool))
            }
        }
        _ => Ok(None),
    }
}
