//! Purpose:
//! Provides shared array-type predicates used by registry builtin checker hooks.
//!
//! Called from:
//! - `crate::builtins::array::count` while validating countable union members.
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
