//! Purpose:
//! Home of the PHP `zval_free` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer and returns `PhpType::Void`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "zval_free",
    area: Pointers,
    params: [zval: Mixed],
    returns: Void,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ZvalFree,
    ),
    summary: "Frees a PHP zval pointer allocated by `zval_pack`.",
    extension: true,
}

/// Validates the zval pointer argument and returns `PhpType::Void`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "zval_free()")?;
    Ok(PhpType::Void)
}
