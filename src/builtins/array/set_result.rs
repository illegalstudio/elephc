//! Purpose:
//! Shares the result typing of the key-preserving set operations: `array_diff`,
//! `array_intersect`, `array_diff_key` and `array_intersect_key`.
//!
//! Called from:
//! - The `check` hooks of those four builtin bindings.
//!
//! Key details:
//! - PHP keeps each survivor's ORIGINAL key (`array_diff([1, 2, 3], [2])` is `[0 => 1, 2 => 3]`),
//!   so an indexed first operand yields an integer-keyed hash, the way `array_unique` does. A
//!   dense indexed result would renumber the survivors (#1645).
//! - The value operations compare by string cast, which the runtime does for scalars and strings.
//!   Other indexed element types (objects, arrays, callables) keep an indexed result and the
//!   legacy identity-comparing helpers; the backend follows the result type declared here.
//! - An associative first operand keeps its own type.

use crate::types::PhpType;

/// Returns the result type of `array_diff`/`array_intersect` for first operand `ty1`.
pub(super) fn value_set_result_type(ty1: PhpType) -> PhpType {
    match &ty1 {
        PhpType::Array(elem)
            if matches!(
                elem.codegen_repr(),
                PhpType::Int
                    | PhpType::Float
                    | PhpType::Bool
                    | PhpType::Str
                    | PhpType::Void
                    | PhpType::Never
            ) =>
        {
            PhpType::AssocArray {
                key: Box::new(PhpType::Int),
                value: elem.clone(),
            }
        }
        _ => ty1,
    }
}

/// Returns the result type of `array_diff_key`/`array_intersect_key` for first operand `ty1`:
/// an indexed operand of any element type becomes an integer-keyed hash, since keys never
/// compare the values.
pub(super) fn key_set_result_type(ty1: PhpType) -> PhpType {
    match ty1 {
        PhpType::Array(elem) => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        },
        other => other,
    }
}
