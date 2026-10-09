//! Purpose:
//! Home of the PHP `array_combine` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: the result is an associative array whose key
//!   type is derived from the keys-array element type (via
//!   `array_key_type_from_value_type`) and whose value type is the values-array element
//!   type, when the typed helper can build it (a list of string keys and a list of one-word
//!   values). Every other array shape, including a declared `array` or a boxed value, is the
//!   boxed PHP array type: the boxed builder converts each key as php does. A check hook is
//!   required because the return type depends on the two inferred argument types.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::boxed_value_may_hold_array;
use crate::types::{array_key_type_from_value_type, PhpType};

builtin! {
    contract: "array_combine",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayCombine,
    ),
}

/// Returns the combined associative-array type for an `array_combine` call.
///
/// The key type is derived from the keys-array element type via
/// `array_key_type_from_value_type`, and the value type is the values-array element
/// type when the typed helper can build the call, and the boxed PHP array type for any other
/// pair of arrays. They are re-inferred here to drive the return type; the registry already
/// inferred them once for side effects, and arity (exactly 2) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let keys_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let vals_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if let (PhpType::Array(key_elem), PhpType::Array(val_elem)) = (&keys_ty, &vals_ty) {
        if typed_helper_reads(key_elem, val_elem) {
            return Ok(PhpType::AssocArray {
                key: Box::new(array_key_type_from_value_type((**key_elem).clone())),
                value: val_elem.clone(),
            });
        }
    }
    for (ty, position) in [(&keys_ty, "first"), (&vals_ty, "second")] {
        if !may_hold_array(ty) {
            return Err(CompileError::new(
                cx.span,
                &format!("array_combine() {position} argument must be array"),
            ));
        }
    }
    Ok(PhpType::php_array())
}

/// Whether the typed helper can build this call: string keys, and values in 8-byte slots.
fn typed_helper_reads(key_elem: &PhpType, val_elem: &PhpType) -> bool {
    let value = val_elem.codegen_repr();
    matches!(
        key_elem.codegen_repr(),
        PhpType::Str | PhpType::Void | PhpType::Never
    ) && (matches!(
        value,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Callable
            | PhpType::Void
            | PhpType::Never
    ) || value.is_refcounted())
}

/// Concrete arrays, the declared `array` type, and boxed values that may hold an array.
fn may_hold_array(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        || ty.is_php_array()
        || boxed_value_may_hold_array(ty)
}
