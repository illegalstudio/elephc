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

use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::super::{Checker, GuardedLocalStorage};
use super::super::narrowing::type_is_definitely_non_null;

/// Gives `name` the stored value's type as its flow type when that type is a strict, non-null
/// member (or subtype) of the union the binding currently holds, keeping the union as storage.
/// `stored_non_null` is the caller's structural proof: a non-null checker type alone is not
/// enough, since a missing array element (`$x = $a[99]`) reads null under a `string` type.
///
/// `function z(DateTimeZone|string $tz): DateTimeZone { $tz = new DateTimeZone('UTC'); return
/// $tz; }` returns a `DateTimeZone`: the store is the newest fact about the value, and php reads
/// it as such. Without this, the merge leaves the union in the environment and the return fails.
pub(super) fn narrow_local_to_stored_value(
    checker: &mut Checker, name: &str, value: &Expr, stored: &PhpType, stored_non_null: bool,
    env: &mut TypeEnv,
) {
    let Some(storage @ PhpType::Union(_)) = env.get(name).cloned() else { return; };
    if checker.type_contains_values(stored, &storage)
        || !(stored_non_null || is_guarded_self_normalisation(checker, name, value, stored, env))
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

/// Accepts `$tz = is_string($tz) ? new DateTimeZone($tz) : $tz;`: each arm is either a proved
/// non-null value or a read of the destination itself, which the condition narrowed. That read
/// is trusted exactly as the `if (is_string($tz)) { ... }` guard complement is at its join.
fn is_guarded_self_normalisation(
    checker: &Checker, name: &str, value: &Expr, stored: &PhpType, env: &TypeEnv,
) -> bool {
    let ExprKind::Ternary { then_expr, else_expr, .. } = &value.kind else { return false; };
    let arm_is_proved = |arm: &Expr| matches!(&arm.kind, ExprKind::Variable(read) if read == name)
        || checker.stored_value_is_non_null(arm, stored, env);
    arm_is_proved(then_expr) && arm_is_proved(else_expr)
}
