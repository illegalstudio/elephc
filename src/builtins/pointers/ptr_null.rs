//! Purpose:
//! Home of the PHP `ptr_null` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` takes no arguments and returns `PhpType::Pointer(None)`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ptr_null",
    area: Pointers,
    params: [],
    arity_error: "ptr_null() takes 0 arguments",
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrNull,
    ),
    summary: "Returns a null raw pointer.",
    extension: true,
}

/// Returns `PhpType::Pointer(None)` unconditionally (no arguments to validate).
///
/// The registry's `check_arity` handles arity enforcement (exactly 0 arguments).
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Pointer(None))
}
