//! Purpose:
//! Shared helpers for stream wrapper/filter registration validation and stream builtin
//! int-argument validation in the io builtin homes.
//! Provides `ensure_int`/`ensure_optional_int` used by `stream_get_contents` and
//! `stream_copy_to_stream`.
//!
//! Called from:
//! - `crate::builtins::io::stream_get_contents` (check hook)
//! - `crate::builtins::io::stream_copy_to_stream` (check hook)
//!
//! Key details:
//! - `ensure_int` and `ensure_optional_int` validate stream builtin length/offset arguments.
//! - There is no class-existence check here any more: php registers a filter class it cannot
//!   find and throws — at RUN TIME — for a wrapper one, so neither belongs to the checker.

use crate::parser::ast::Expr;
use crate::errors::CompileError;
use crate::types::{PhpType, TypeEnv};
use crate::types::checker::Checker;

/// Ensures a stream builtin argument is an `int`, emitting a parameter-specific
/// compile error otherwise.
pub(crate) fn ensure_int(
    checker: &mut Checker,
    builtin: &str,
    param: &str,
    arg: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let ty = checker.infer_type(arg, env)?;
    if accepts_int(&ty) {
        return Ok(());
    }
    Err(CompileError::new(
        arg.span,
        &format!("{}() {} must be int", builtin, param),
    ))
}

/// Ensures a stream builtin length argument is `int|null`, matching PHP's
/// nullable `$length` parameter while keeping codegen from seeing strings/floats.
pub(crate) fn ensure_optional_int(
    checker: &mut Checker,
    builtin: &str,
    param: &str,
    arg: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let ty = checker.infer_type(arg, env)?;
    if accepts_int_or_null(&ty) {
        return Ok(());
    }
    Err(CompileError::new(
        arg.span,
        &format!("{}() {} must be int or null", builtin, param),
    ))
}

/// Returns true when a type is statically compatible with an `int` parameter.
fn accepts_int(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int => true,
        PhpType::Union(members) => members.iter().all(accepts_int),
        _ => false,
    }
}

/// Returns true when a type is statically compatible with an `int|null` parameter.
fn accepts_int_or_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Void => true,
        PhpType::Union(members) => members.iter().all(accepts_int_or_null),
        _ => false,
    }
}
