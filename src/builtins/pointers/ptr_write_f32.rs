//! Purpose:
//! Home of the PHP `ptr_write_f32` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the pointer and a numeric value argument and returns `PhpType::Void`.
//! - Integer values are converted to double before the 32-bit IEEE 754 store; `mixed` values
//!   are cast at runtime.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "ptr_write_f32",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrWriteF32,
    ),
}

/// Validates the pointer and numeric value arguments and returns `PhpType::Void`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// The value must be a float, an int (converted to float), or a runtime-typed `mixed` value.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, &format!("{}()", cx.name))?;
    let value_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(
        value_ty,
        PhpType::Float | PhpType::Int | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() value must be float", cx.name),
        ));
    }
    Ok(PhpType::Void)
}
