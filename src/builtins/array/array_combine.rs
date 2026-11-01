//! Purpose:
//! Home of the PHP `array_combine` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` derives the result key/value types from either indexed or associative array
//!   operands. Gradual unions are accepted when they contain an array arm and are checked at
//!   runtime. A check hook is required because the return type depends on both arguments.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
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
/// type. Indexed, associative, and gradual array operands are accepted. They are re-inferred
/// here to drive the return type; the registry already inferred them once for side effects,
/// and arity (exactly 2) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let keys_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let vals_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let key_elem = array_combine_element_type(keys_ty).ok_or_else(|| {
        CompileError::new(cx.span, "array_combine() first argument must be array")
    })?;
    let val_elem = array_combine_element_type(vals_ty).ok_or_else(|| {
        CompileError::new(cx.span, "array_combine() second argument must be array")
    })?;
    Ok(PhpType::AssocArray {
        key: Box::new(array_key_type_from_value_type(key_elem)),
        value: Box::new(val_elem),
    })
}

/// Returns the element type exposed by an array-shaped operand, including gradual unions.
fn array_combine_element_type(ty: PhpType) -> Option<PhpType> {
    match ty {
        PhpType::Array(element) => Some(*element),
        PhpType::AssocArray { value, .. } => Some(*value),
        PhpType::Mixed => Some(PhpType::Mixed),
        PhpType::Union(members) => {
            let mut merged: Option<PhpType> = None;
            for member in members {
                let Some(element) = array_combine_element_type(member) else {
                    continue;
                };
                merged = Some(match merged {
                    Some(previous) if previous.codegen_repr() == element.codegen_repr() => previous,
                    Some(_) => PhpType::Mixed,
                    None => element,
                });
            }
            merged
        }
        _ => None,
    }
}
