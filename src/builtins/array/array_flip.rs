//! Purpose:
//! Home of the PHP `array_flip` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: flipping swaps keys and values, so the
//!   result is an associative array whose key type is derived from the input value
//!   type (via `array_key_type_from_value_type`). An indexed array flips to
//!   `AssocArray<key-from-elem, Int>`; an associative array flips to
//!   `AssocArray<key-from-value, old-key>`. A check hook is required because the
//!   return type depends on the inferred argument type.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::{array_key_type_from_value_type, PhpType};

builtin! {
    name: "array_flip",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayFlip,
    ),
    summary: "Exchanges all keys with their associated values in an array.",
    php_manual: "https://www.php.net/manual/en/function.array-flip.php",
}

/// Returns the flipped associative-array type for an `array_flip` call.
///
/// Keys and values swap places, so the new key type is derived from the old value
/// type via `array_key_type_from_value_type`. The argument is re-inferred here to
/// drive the return type; the registry already inferred it once for side effects,
/// and arity is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(elem_ty) => Ok(PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(*elem_ty)),
            value: Box::new(PhpType::Int),
        }),
        PhpType::AssocArray { key, value } => Ok(PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(*value)),
            value: key,
        }),
        _ => Err(CompileError::new(
            cx.span,
            "array_flip() argument must be array",
        )),
    }
}
