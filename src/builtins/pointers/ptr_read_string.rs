//! Purpose:
//! Home of the PHP `ptr_read_string` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a pointer and the second is an integer
//!   length, and returns `PhpType::Str`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ptr_read_string",
    area: Pointers,
    params: [pointer: Mixed, length: Mixed],
    returns: Str,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrReadString,
    ),
    summary: "Copies raw bytes from a pointer into a PHP string of the given length.",
    extension: true,
}

/// Validates pointer and integer length arguments and returns `PhpType::Str`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
pub(crate) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "ptr_read_string()")?;
    let len_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if len_ty != PhpType::Int {
        return Err(CompileError::new(
            cx.span,
            "ptr_read_string() length must be int",
        ));
    }
    Ok(PhpType::Str)
}
