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
//! - The value operations compare by string cast. Scalars and strings take
//!   `__rt_hash_value_diff_intersect`; every other element type (boxed values, objects, arrays,
//!   callables) takes `__rt_array_set_op_boxed`, which renders each element with
//!   `__rt_mixed_cast_string` (`__toString` for an object). Both keep the original keys, so an
//!   indexed first operand of any element type yields an integer-keyed hash.
//! - An associative first operand keeps its own type; a declared `array` or a boxed value that may
//!   hold one answers the boxed PHP array type.

use crate::types::PhpType;

/// Returns the result type of `array_diff`/`array_intersect` for first operand `ty1`.
pub(super) fn value_set_result_type(ty1: PhpType) -> PhpType {
    match ty1 {
        PhpType::Array(elem) => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        },
        PhpType::AssocArray { .. } => ty1,
        _ => PhpType::php_array(),
    }
}

/// Concrete arrays, the declared `array` type, and boxed values that may hold an array: the
/// operands a by-value set operation accepts.
pub(super) fn value_set_operand_may_hold_array(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        || ty.is_php_array()
        || crate::types::checker::builtins::arrays::boxed_value_may_hold_array(ty)
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
