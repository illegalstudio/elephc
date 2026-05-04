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
