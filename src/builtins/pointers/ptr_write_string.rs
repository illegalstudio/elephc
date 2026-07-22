//! Purpose:
//! Home of the PHP `ptr_write_string` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates pointer and string arguments and returns `PhpType::Int`
//!   (the number of bytes written).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ptr_write_string",
    area: Pointers,
    params: [pointer: Mixed, string: Mixed],
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrWriteString,
    ),
    summary: "Copies PHP string bytes into raw memory at the given pointer.",
    extension: true,
}

/// Validates pointer and string arguments and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// Returns the number of bytes written as an integer.
pub(crate) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "ptr_write_string()")?;
    let str_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if str_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.span,
            "ptr_write_string() string argument must be string",
        ));
    }
    Ok(PhpType::Int)
}
