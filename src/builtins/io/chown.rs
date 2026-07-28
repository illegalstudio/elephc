//! Purpose:
//! Home of the PHP `chown` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `user` argument to be `Int` or `Str`
//!   (a numeric UID or a user name), emitting the diagnostic at that argument's span.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "chown",
    area: Io,
    params: [filename: Str, user: Mixed],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Chown,
    ),
    summary: "Changes file owner.",
    php_manual: "function.chown",
}

/// Returns `Bool`, rejecting a `user` argument that is neither `Int` nor `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let principal_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(principal_ty, PhpType::Int | PhpType::Str) {
        return Err(CompileError::new(
            cx.args[1].span,
            &format!("{}() owner/group must be int or string", cx.name),
        ));
    }
    Ok(PhpType::Bool)
}
