//! Purpose:
//! Home of the PHP `touch` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` delegates to the relocated `check_touch` helper, which validates that
//!   the optional `mtime`/`atime` timestamp arguments are `int` or `null` and that
//!   `mtime` is not `null` when `atime` is provided.
//! - `arity_error` is overridden to preserve the legacy message
//!   "touch() takes 1, 2, or 3 arguments" (the registry default for a 1-required,
//!   3-max builtin produces "1 to 3 arguments").

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::checker::Checker;
use crate::types::{PhpType, TypeEnv};

builtin! {
    contract: "touch",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Touch,
    ),
}

/// Returns `Bool` after validating `touch()` timestamp arguments via `check_touch`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_touch(cx.checker, cx.args, cx.span, cx.env)
}

/// Returns whether a type may reach `touch()`'s declared `?int` timestamp parameter.
///
/// GRADUAL is accepted, because that is what an ordinary int expression is here: `time() + 100`
/// types as `Mixed`, since PHP promotes an overflowing int addition to float and the checker
/// models that. The contract declares `mtime`/`atime` as `?int`, so lowering narrows the boxed
/// value at the call boundary exactly as it does for `date('Y', time() + 100)` — which compiles
/// and runs today, while `touch()` alone refused, from a hand-written test that predates that
/// path. Symfony's `FilesystemCommonTrait::write()` writes
/// `touch($tmp, $expiresAt ?: time() + 31556952)`.
///
/// `Float` rides along for the same reason it does at any declared `int` boundary; a type that
/// is not numeric at all — a string, an array, an object — still gets the diagnostic.
fn timestamp_type_is_accepted(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Float | PhpType::Void | PhpType::Mixed | PhpType::Never => true,
        PhpType::Union(members) => members.iter().all(timestamp_type_is_accepted),
        _ => false,
    }
}

/// Validates `touch()` arity (1–3 args) and timestamp argument types.
/// Timestamp args must be `int` (a Unix timestamp) or `null` (omit to use current time).
///
/// # Errors
/// Returns an error if:
/// - Arity is 0 or greater than 3
/// - Any timestamp arg is neither `int` nor `null`
/// - `atime` is `null` but `mtime` is non-null (atime implies current time, so mtime cannot be set separately)
///
/// # Returns
/// `Ok(PhpType::Bool)` on success.
fn check_touch(
    checker: &mut Checker,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    if args.is_empty() || args.len() > 3 {
        return Err(CompileError::new(span, "touch() takes 1, 2, or 3 arguments"));
    }
    checker.infer_type(&args[0], env)?;
    let mut timestamp_types = Vec::new();
    for arg in args.iter().skip(1) {
        let ty = checker.infer_type(arg, env)?;
        if !timestamp_type_is_accepted(&ty) {
            return Err(CompileError::new(
                arg.span,
                "touch() timestamp arguments must be int or null",
            ));
        }
        timestamp_types.push(ty);
    }
    if matches!(timestamp_types.first(), Some(PhpType::Void))
        && matches!(timestamp_types.get(1), Some(ty) if !matches!(ty, PhpType::Void))
    {
        return Err(CompileError::new(
            span,
            "touch() mtime cannot be null when atime is provided",
        ));
    }
    Ok(PhpType::Bool)
}
