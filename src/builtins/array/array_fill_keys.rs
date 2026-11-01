//! Purpose:
//! Home of the PHP `array_fill_keys` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` accepts concrete indexed arrays and gradual array values, returning an
//!   associative array whose key type is derived from the known element type when
//!   possible and remains `Mixed` for runtime-only shapes.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_fill_keys",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayFillKeys,
    ),
}

/// Validates `keys` is an indexed or gradual array and returns the resulting assoc-array type.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// The key type of the resulting assoc array is derived via `array_key_type_from_value_type`
/// from the element type of `keys`; the value type is the inferred type of `value`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let keys_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let val_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let (key_elem, result_value_ty) = match keys_ty {
        PhpType::Array(elem) => (*elem, val_ty),
        PhpType::Mixed | PhpType::Union(_) => (PhpType::Mixed, PhpType::Mixed),
        _ => {
            return Err(CompileError::new(
                cx.span,
                "array_fill_keys() first argument must be array",
            ));
        }
    };
    Ok(PhpType::AssocArray {
        key: Box::new(crate::types::array_key_type_from_value_type(key_elem)),
        value: Box::new(result_value_ty),
    })
}
