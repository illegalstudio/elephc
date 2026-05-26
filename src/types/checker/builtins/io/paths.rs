//! Purpose:
//! Type-checks PHP IO builtin paths helpers and signatures.
//! Validates arity, argument categories, resource handling, and return types before codegen sees calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::io::check_builtin()`
//!
//! Key details:
//! - Return types and diagnostics must stay aligned with `crate::types::signatures` and builtin codegen emitters.

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::common::BuiltinResult;
use super::super::super::Checker;

/// Type-checks path builtins (`basename`, `dirname`, `fnmatch`, `realpath`, `pathinfo`)
/// and returns the return `PhpType` on success, `BuiltinResult` with diagnostic on failure.
///
/// Arity and argument types are validated; `checker.infer_type()` is called on each argument
/// to populate type information. Returns `Ok(None)` for unrecognized names so callers can fall through.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "basename" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "basename() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "dirname" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "dirname() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if matches!(args.get(1).map(|arg| &arg.kind), Some(ExprKind::IntLiteral(levels)) if *levels < 1)
            {
                return Err(CompileError::new(
                    span,
                    "dirname() levels must be greater than or equal to 1",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "fnmatch" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "fnmatch() takes 2 or 3 arguments"));
            }
            for arg in &args[..2] {
                checker.infer_type(arg, env)?;
            }
            if let Some(flags) = args.get(2) {
                let flags_ty = checker.infer_type(flags, env)?;
                if flags_ty != PhpType::Int {
                    return Err(CompileError::new(span, "fnmatch() flags must be int"));
                }
            }
            Ok(Some(PhpType::Bool))
        }
        "realpath" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "realpath() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Union(vec![PhpType::Str, PhpType::Bool])))
        }
        "pathinfo" => check_pathinfo(checker, args, span, env).map(Some),
        _ => Ok(None),
    }
}

/// Validates `pathinfo()` arity (1–2 args), checks the optional flag is `int`,
/// and returns `string` for a known flag, `assoc-array<string,string>` when no flag
/// or `PATHINFO_ALL` (15) is given, or a union in the mixed case.
///
/// Calls `pathinfo_static_flag_value()` to resolve `PATHINFO_*` constants at compile time.
/// Falls back to a union type when the flag is dynamic or missing.
fn check_pathinfo(
    checker: &mut Checker,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(span, "pathinfo() takes 1 or 2 arguments"));
    }
    checker.infer_type(&args[0], env)?;
    let flag = match args.get(1) {
        Some(flag) => {
            let flag_ty = checker.infer_type(flag, env)?;
            if flag_ty != PhpType::Int {
                return Err(CompileError::new(flag.span, "pathinfo() flag must be int"));
            }
            pathinfo_static_flag_value(flag)
        }
        None => None,
    };
    if args.get(1).is_none() || flag == Some(15) {
        Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        })
    } else if flag.is_none() {
        Ok(checker.normalize_union_type(vec![
            PhpType::Str,
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Str),
            },
        ]))
    } else {
        Ok(PhpType::Str)
    }
}

/// Extracts a literal `PATHINFO_*` constant value from `flag` expression at compile time.
///
/// Handles integer literals, `PATHINFO_*` constants (`PATHINFO_DIRNAME`=1, `PATHINFO_BASENAME`=2,
/// `PATHINFO_EXTENSION`=4, `PATHINFO_FILENAME`=8, `PATHINFO_ALL`=15), negation, and bitwise
/// combinators (`|`, `&`, `^`). Returns `None` for non-static expressions (variables, function
/// calls, etc.) so `check_pathinfo` can fall back to a union type.
fn pathinfo_static_flag_value(flag: &Expr) -> Option<i64> {
    match &flag.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str() {
            "PATHINFO_DIRNAME" => Some(1),
            "PATHINFO_BASENAME" => Some(2),
            "PATHINFO_EXTENSION" => Some(4),
            "PATHINFO_FILENAME" => Some(8),
            "PATHINFO_ALL" => Some(15),
            _ => None,
        },
        ExprKind::Negate(inner) => pathinfo_static_flag_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = pathinfo_static_flag_value(left)?;
            let right = pathinfo_static_flag_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
