//! Purpose:
//! Home of the PHP `ptr_is_null` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer type and returns `PhpType::Bool`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ptr_is_null",
    area: Pointers,
    params: [pointer: Mixed],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrIsNull,
    ),
    summary: "Returns true if the pointer is null.",
    extension: true,
}

/// Validates that the argument is a pointer type and returns `PhpType::Bool`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
pub(crate) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ty, cx.span, "ptr_is_null()")?;
    Ok(PhpType::Bool)
}
