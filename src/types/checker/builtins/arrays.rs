//! Purpose:
//! Provides shared array-type predicates used by registry builtin checker hooks.
//!
//! Called from:
//! - `crate::builtins::array::count` while validating countable union members.
//! - `crate::builtins::array::{array_values, array_flip, in_array}` while accepting a boxed
//!   receiver whose array-ness is only known at run time.
//!
//! Key details:
//! - `Mixed` remains countable because runtime tags decide the concrete container shape.

use crate::types::PhpType;

/// Returns `true` if a `PhpType` is a countable array type for Union membership checks.
///
/// Used by `crate::builtins::array::count` to test whether ANY branch of a Union type is
/// countable, in which case `count()` returns `Int` for the whole union and a non-countable
/// branch raises PHP's `TypeError` at run time, as PHP does.
pub(crate) fn union_member_is_countable_array(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    )
}

/// Returns `true` when a receiver's static type is boxed and MAY hold an array at run time.
///
/// That is `mixed` itself (a `json_decode()` result, an untyped value) or a union with at least
/// one array-capable member (`array|false`). Both lower as a boxed Mixed cell, so the backend's
/// boxed path opens the box, dispatches on the runtime tag, and raises PHP's `TypeError` when the
/// payload is not an array. A union with no array member can never succeed and stays a compile
/// error, exactly as `count()` treats it.
pub(crate) fn boxed_value_may_hold_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(union_member_is_countable_array),
        _ => false,
    }
}
