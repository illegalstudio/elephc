//! Purpose:
//! Home of the PHP `array_fill_keys` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns an associative array whose key type is derived from the element type of
//!   `keys` and whose value type matches `value` when the typed helper can build it (a list of
//!   string keys, a one-word value). Every other array shape, including a declared `array` or a
//!   boxed value, is the boxed PHP array type: the boxed builder converts each key as php does.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::boxed_value_may_hold_array;
use crate::types::PhpType;

builtin! {
    contract: "array_fill_keys",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayFillKeys,
    ),
}

/// Validates `keys` is an indexed array and returns the resulting assoc-array type.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// The key type of the resulting assoc array is derived via `array_key_type_from_value_type`
/// from the element type of `keys`; the value type is the inferred type of `value`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let keys_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let val_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if let PhpType::Array(elem) = &keys_ty {
        if typed_helper_reads(elem, &val_ty) {
            return Ok(PhpType::AssocArray {
                key: Box::new(crate::types::array_key_type_from_value_type((**elem).clone())),
                value: Box::new(val_ty),
            });
        }
    }
    if matches!(keys_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        || keys_ty.is_php_array()
        || boxed_value_may_hold_array(&keys_ty)
    {
        return Ok(PhpType::php_array());
    }
    Err(CompileError::new(
        cx.span,
        "array_fill_keys() first argument must be array",
    ))
}

/// Whether the typed helper can build this call: a list of string keys and a one-word value.
/// Every other shape (integer or boxed keys, a string value, associative or boxed keys) goes
/// through the boxed builder, which converts each key the way php does.
fn typed_helper_reads(key_elem: &PhpType, value: &PhpType) -> bool {
    matches!(
        key_elem.codegen_repr(),
        PhpType::Str | PhpType::Void | PhpType::Never
    ) && matches!(
        value.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}
