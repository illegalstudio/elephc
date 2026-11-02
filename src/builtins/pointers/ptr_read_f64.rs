//! Purpose:
//! Home of the PHP `ptr_read_f64` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer type and returns `PhpType::Float`.
//! - The backend widens the 64-bit IEEE 754 value to the PHP double representation.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "ptr_read_f64",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrReadF64,
    ),
}

/// Validates that the argument is a pointer type and returns `PhpType::Float`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ty, cx.span, &format!("{}()", cx.name))?;
    Ok(PhpType::Float)
}
