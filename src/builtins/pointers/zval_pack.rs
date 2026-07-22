//! Purpose:
//! Home of the PHP `zval_pack` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` accepts any value and returns `PhpType::Pointer(None)`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "zval_pack",
    area: Pointers,
    params: [value: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ZvalPack,
    ),
    summary: "Packs an elephc runtime value into a heap-allocated PHP zval pointer.",
    extension: true,
}

/// Accepts any value and returns an untyped raw pointer to the allocated zval.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Pointer(None))
}
