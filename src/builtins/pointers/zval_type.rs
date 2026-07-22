//! Purpose:
//! Home of the PHP `zval_type` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer and returns `PhpType::Int`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "zval_type",
    area: Pointers,
    params: [zval: Mixed],
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ZvalType,
    ),
    summary: "Returns the PHP zval type byte for a zval pointer.",
    extension: true,
}

/// Validates the zval pointer argument and returns `PhpType::Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "zval_type()")?;
    Ok(PhpType::Int)
}
