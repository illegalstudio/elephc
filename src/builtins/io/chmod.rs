//! Purpose:
//! Home of the PHP `chmod` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `permissions` argument to be `Int`,
//!   emitting the diagnostic at the mode argument's span.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "chmod",
    area: Io,
    params: [filename: Str, permissions: Int],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Chmod,
    ),
    summary: "Changes file mode.",
    php_manual: "function.chmod",
}

/// Returns `Bool`, rejecting a non-`Int` `permissions` argument at its own span.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let mode_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if mode_ty != PhpType::Int {
        return Err(CompileError::new(cx.args[1].span, "chmod() mode must be int"));
    }
    Ok(PhpType::Bool)
}
