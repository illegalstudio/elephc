//! Purpose:
//! Home of the PHP `ptr_read8` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer type and returns `PhpType::Int`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "ptr_read8",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrRead8,
    ),
}

/// Validates that the argument is a pointer type and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
pub(super) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ty, cx.span, &format!("{}()", cx.name))?;
    Ok(PhpType::Int)
}
