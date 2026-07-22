//! Purpose:
//! Home of the PHP `ptr_get` builtin: its single-source registry declaration and semantic target.
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
    name: "ptr_get",
    area: Pointers,
    params: [pointer: Mixed],
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrGet,
    ),
    summary: "Reads one machine word through a raw pointer and returns it as an integer.",
    extension: true,
}

/// Validates that the argument is a pointer type and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ty, cx.span, &format!("{}()", cx.name))?;
    Ok(PhpType::Int)
}
