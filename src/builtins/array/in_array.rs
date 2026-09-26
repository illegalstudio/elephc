//! Purpose:
//! Home of the PHP `in_array` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the second argument is an array and returns `Bool`. A `mixed`
//!   haystack or a union with an array member is accepted: the boxed membership scan opens the
//!   box at run time and raises PHP's `TypeError` for a non-array payload.
//! - The optional `strict` (3rd) argument selects PHP `===` membership; omitted or
//!   false strictness uses PHP `==` semantics, including boxed array parameters.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::boxed_value_may_hold_array;
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
    if !arr_ty.is_php_array()
        && !boxed_value_may_hold_array(&arr_ty)
        && !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        return Err(CompileError::new(
            cx.span,
            "in_array() second argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}
