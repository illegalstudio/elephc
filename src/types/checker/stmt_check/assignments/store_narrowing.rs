//! Purpose:
//! Narrows a local's read view to the value an ordinary `=` store just put in it.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments::locals::check_assign_value_type`
//!
//! Key details:
//! - The union the binding had before the store stays its storage contract in
//!   `guarded_union_types`, exactly as a coalescing store keeps it, so codegen still sees the
//!   slot it allocated and a later store of a wider member is measured against the union.
//! - Only stable locals qualify: by-reference parameters, reference-aliased or by-reference
//!   captured locals, globals, statics, boxed `Mixed` frames and program globals keep their
//!   declared view, since something outside this flow may write them.

use crate::types::{PhpType, TypeEnv};

use super::super::super::{Checker, GuardedLocalStorage};
use super::super::narrowing::type_is_definitely_non_null;

/// Gives `name` the stored value's type as its flow type when that type is a strict, non-null
/// member (or subtype) of the union the binding currently holds, keeping the union as storage.
///
/// `function z(DateTimeZone|string $tz): DateTimeZone { $tz = new DateTimeZone('UTC'); return
/// $tz; }` returns a `DateTimeZone`: the store is the newest fact about the value, and php reads
/// it as such. Without this, the merge leaves the union in the environment and the return fails.
pub(super) fn narrow_local_to_stored_value(
    checker: &mut Checker, name: &str, stored: &PhpType, env: &mut TypeEnv,
) {
    let Some(storage @ PhpType::Union(_)) = env.get(name).cloned() else { return; };
    if checker.type_contains_values(stored, &storage)
        || !type_is_definitely_non_null(stored)
        || matches!(stored, PhpType::Mixed | PhpType::TaggedScalar | PhpType::Callable)
        || !checker.local_flow_read_is_stable(name)
        || checker.merged_assignment_type(&storage, stored).as_ref() != Some(&storage)
    {
        return;
    }
    checker.guarded_union_types.entry(name.to_string())
        .or_insert(GuardedLocalStorage { ty: storage, coalesced: false });
    env.insert(name.to_string(), stored.clone());
}
