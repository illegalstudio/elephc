//! Purpose:
//! Type-checks the io PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

mod common;
mod debug;
mod files;
mod paths;
mod stats;
mod streams;

use super::super::Checker;
use crate::parser::ast::Expr;
use crate::types::TypeEnv;

use common::BuiltinResult;

/// Type-checks a builtin call by delegating to the appropriate I/O subsystem checker.
///
/// Checks `debug`, `streams`, `stats`, `files`, and `paths` submodules in order.
/// Returns `Ok(Some(result))` if the builtin was recognized by a subsystem,
/// `Ok(None)` if no subsystem handles the name, or an error if validation fails.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    if let Some(result) = debug::check_builtin(checker, name, args, span, env)? {
        return Ok(Some(result));
    }
    if let Some(result) = streams::check_builtin(checker, name, args, span, env)? {
        return Ok(Some(result));
    }
    if let Some(result) = stats::check_builtin(checker, name, args, span, env)? {
        return Ok(Some(result));
    }
    if let Some(result) = files::check_builtin(checker, name, args, span, env)? {
        return Ok(Some(result));
    }
    if let Some(result) = paths::check_builtin(checker, name, args, span, env)? {
        return Ok(Some(result));
    }
    Ok(None)
}
