//! Purpose:
//! Home of the PHP `in_array` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the second argument is an array and returns `Bool`.
//! - The optional `strict` (3rd) argument selects PHP `===` membership; omitted or
//!   false strictness uses PHP `==` semantics for the supported scalar/string paths.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "in_array",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::InArray,
    ),
}

/// Validates that the second argument is an array and returns `Bool`.
///
/// The registry's `check_arity` handles the 2-to-3 argument range. This hook validates
/// that `haystack` is an array and returns the `Bool` return type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    // An `array|false` union — scandir(), glob(), file() — is accepted by reading through to
    // its array member: the argument lowering pairs this with an unbox-or-throw, so a runtime
    // `false` raises php's TypeError rather than compiling `in_array($x, scandir($d))` away.
    let arr_ty = arr_ty.array_or_false_member().cloned().unwrap_or(arr_ty);
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "in_array() second argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}
