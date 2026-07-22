//! Purpose:
//! Home of the PHP `array_fill_keys` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is an indexed array and returns an
//!   associative array whose key type is derived from the element type of `keys`
//!   and whose value type matches `value`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_fill_keys",
    area: Array,
    params: [keys: Mixed, value: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayFillKeys,
    ),
    summary: "Fill an array with values, specifying keys.",
    php_manual: "https://www.php.net/manual/en/function.array-fill-keys.php",
}

/// Validates `keys` is an indexed array and returns the resulting assoc-array type.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// The key type of the resulting assoc array is derived via `array_key_type_from_value_type`
/// from the element type of `keys`; the value type is the inferred type of `value`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let keys_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let val_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let key_elem = match keys_ty {
        PhpType::Array(elem) => *elem,
        _ => {
            return Err(CompileError::new(
                cx.span,
                "array_fill_keys() first argument must be array",
            ));
        }
    };
    Ok(PhpType::AssocArray {
        key: Box::new(crate::types::array_key_type_from_value_type(key_elem)),
        value: Box::new(val_ty),
    })
}
