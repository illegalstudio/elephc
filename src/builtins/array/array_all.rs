//! Purpose:
//! Home of the PHP `array_all` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Packed and associative arrays share runtime value/key iteration and callback validation.
//! - The shared checker preserves declared callback types when array elements are unknown.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_all",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayAll,
    ),
}

/// Validates the array and keyed predicate, retaining the builtin's declared result type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::predicate::check(cx)?;
    Ok(PhpType::Bool)
}
