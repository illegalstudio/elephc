//! Purpose:
//! Home of the PHP `ptr_set` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates pointer and word-sized value arguments and returns `PhpType::Void`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ptr_set",
    area: Pointers,
    params: [pointer: Mixed, value: Mixed],
    returns: Void,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrSet,
    ),
    summary: "Writes one machine word through a raw pointer.",
    extension: true,
}

/// Validates pointer and word-compatible value arguments and returns `PhpType::Void`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// The value argument must be a word-pointer-compatible type (int, bool, pointer, etc.).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "ptr_set()")?;
    let value_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    cx.checker.ensure_word_pointer_value(&value_ty, cx.span)?;
    Ok(PhpType::Void)
}
