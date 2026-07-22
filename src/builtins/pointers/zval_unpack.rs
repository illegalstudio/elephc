//! Purpose:
//! Home of the PHP `zval_unpack` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer and returns `PhpType::Mixed`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "zval_unpack",
    area: Pointers,
    params: [zval: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ZvalUnpack,
    ),
    summary: "Unpacks a PHP zval pointer into an owned elephc Mixed value.",
    extension: true,
}

/// Validates the zval pointer argument and returns `PhpType::Mixed`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "zval_unpack()")?;
    Ok(PhpType::Mixed)
}
