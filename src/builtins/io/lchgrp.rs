//! Purpose:
//! Home of the PHP `lchgrp` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `group` argument to be `Int` or `Str`.
//!   `lchgrp` changes the group of a symlink itself rather than its target.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "lchgrp",
    area: Io,
    params: [filename: Str, group: Str],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Lchgrp,
    ),
    summary: "Changes group ownership of a symlink.",
    php_manual: "function.lchgrp",
}

/// Returns `Bool`, rejecting a `group` argument that is neither `Int` nor `Str`.
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
