//! Purpose:
//! Null coalesce and ternary-like lazy branch lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers null coalesce so the default expression is evaluated only for null values.
pub(super) fn lower_null_coalesce(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let value = lower_null_coalesce_value(ctx, value);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let split_initialized = ctx.initialized_slots_snapshot();
    let default_block = ctx
        .builder
        .create_named_block("coalesce.default", Vec::new());
    let value_block = ctx.builder.create_named_block("coalesce.value", Vec::new());
    let merge = ctx.builder.create_named_block("coalesce.merge", Vec::new());
    let join = crate::ir_lower::stmt::LazyBranchJoin::begin(ctx, merge);
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: default_block,
        then_args: Vec::new(),
        else_target: value_block,
        else_args: Vec::new(),
    });

    // Both arms are parked and the merge temp is typed from their lowered values (see
    // `PendingBranchValue`): the default arm's type is only known once it is lowered.
    ctx.builder.position_at_end(default_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    join.enter_edge(ctx);
    let default_value = lower_expr(ctx, default);
    let default_branch = park_pending_branch(ctx, default_value, Some(value));

    ctx.builder.position_at_end(value_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    join.enter_edge(ctx);
    // This arm only runs for a non-null value, so its null member does not join the merge.
    let value_branch = park_pending_branch(ctx, value, None).map(PendingBranchValue::excluding_null);

    lower_two_way_pending_merge(ctx, default_branch, value_branch, merge, &split_initialized, Some(join), expr.span)
}

/// Stores two parked lazy-branch values into a merge temp typed from both, joins the
/// definitely-initialized slot sets at `merge` and takes the temp's value there.
pub(super) fn lower_two_way_pending_merge(
    ctx: &mut LoweringContext<'_, '_>,
    first: Option<PendingBranchValue>,
    second: Option<PendingBranchValue>,
    merge: BlockId,
    split_initialized: &HashSet<LocalSlotId>,
    mut join: Option<crate::ir_lower::stmt::LazyBranchJoin>,
    span: Span,
) -> LoweredValue {
    let first_reachable = first.is_some();
    let second_reachable = second.is_some();
    let (temp_name, branches) =
        store_pending_branches(ctx, first.into_iter().chain(second).collect(), merge, None, join.as_mut(), span);
    let mut stored = branches.iter().map(|branch| branch.initialized().clone());
    let first_initialized = if first_reachable { stored.next().unwrap_or_default() } else { HashSet::new() };
    let second_initialized = if second_reachable { stored.next().unwrap_or_default() } else { HashSet::new() };
    match join {
        Some(join) => join.finish(ctx, span),
        None => ctx.builder.position_at_end(merge),
    }
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        split_initialized,
        first_initialized,
        first_reachable,
        second_initialized,
        second_reachable,
    ));
    take_owned_temp(ctx, &temp_name, span)
}

/// Lowers the value side of `??`, suppressing undefined-offset warnings from
/// native array reads while preserving nullsafe-chain lazy evaluation.
pub(super) fn lower_null_coalesce_value(ctx: &mut LoweringContext<'_, '_>, value: &Expr) -> LoweredValue {
    if let Some(value) = nullsafe_chain::lower_with_missing_warning(ctx, value, false) {
        return value;
    }
    if let ExprKind::ArrayAccess { array, index } = &value.kind {
        return lower_array_access_with_missing_warning(ctx, array, index, value, false);
    }
    // A typed property with no default starts UNINITIALIZED, and an ordinary read of one is
    // fatal in PHP. `??` is precisely the construct that must not raise there, so a property
    // that can be in that state is read the way `isset()` reads it. Every other property keeps
    // the ordinary path and its exact slot type.
    if let ExprKind::PropertyAccess { object, property } = &value.kind {
        // The left operand of `??` is one of php's silent probes, exactly like `isset()`: a name
        // this scope may not reach answers the DEFAULT rather than raising, and a name that was
        // never created answers it without an `Undefined property` warning. A receiver whose
        // class is only known at run time needs the runtime-name form to carry that mode.
        let needs_probe = property_probe_needs_runtime_name_form(ctx, object);
        let object = lower_expr(ctx, object);
        if property_can_be_uninitialized(ctx, object.value, property) {
            return lower_initialized_property_value(ctx, object, property, value);
        }
        if needs_probe {
            return lower_property_probe_from_value(ctx, object, property, value);
        }
        return lower_property_get_from_value(ctx, object, property, Op::PropGet, value);
    }
    if let ExprKind::DynamicPropertyAccess { object, property } = &value.kind {
        return lower_dynamic_property_fetch(ctx, object, property, PropertyFetchMode::Probe, value);
    }
    // A typed STATIC property starts uninitialized the same way, and its guard lives in the
    // backend rather than in an operation the lowering could branch on — so `??` needs its own
    // probe here too. `S::$s ?? "d"` raised where PHP answers `d`.
    if let ExprKind::StaticPropertyAccess { receiver, property } = &value.kind {
        if static_property_can_be_uninitialized(ctx, receiver, property) {
            return lower_initialized_static_property_value(ctx, receiver, property, value);
        }
    }
    lower_expr(ctx, value)
}

/// Returns the materialized result type for a null-coalesce merge.
pub(super) fn null_coalesce_result_type(
    ctx: &LoweringContext<'_, '_>,
    value: ValueId,
    default: &Expr,
) -> PhpType {
    let value_ty = strip_void_from_union(ctx.builder.value_php_type(value)).codegen_repr();
    let default_ty = materialized_expr_type_for_merge(ctx, default).codegen_repr();
    // `$o ?? null` can still be null. An object or callable slot has no null of its own, so a
    // default that may be null keeps the merge nullable (boxed) instead of letting the non-null
    // side's slot type win and then reading the stored null back as an object.
    if matches!(value_ty, PhpType::Object(_) | PhpType::Callable) && php_type_allows_null(&default_ty) {
        return nullable_aware_branch_merge_type(&value_ty, &default_ty);
    }
    wider_type_for_merge(&value_ty, &default_ty)
}

/// Chooses the wider materialized type for branch-local merge storage.
pub(super) fn wider_type_for_merge(left: &PhpType, right: &PhpType) -> PhpType {
    let left = left.codegen_repr();
    let right = right.codegen_repr();
    if left == right {
        return left;
    }
    if matches!(left, PhpType::Void | PhpType::Never) {
        return right;
    }
    if matches!(right, PhpType::Void | PhpType::Never) {
        return left;
    }
    match (&left, &right) {
        // Mismatched element types must widen elementwise (issue #549): letting
        // one side win wholesale relabels the other side's runtime slots, so
        // typed reads through the merged type misinterpret the payload bytes.
        (PhpType::Array(left_elem), PhpType::Array(right_elem)) => {
            PhpType::Array(Box::new(merge_ir_indexed_element_type(
                (**left_elem).clone(),
                (**right_elem).clone(),
            )))
        }
        (
            PhpType::AssocArray { key: left_key, value: left_value },
            PhpType::AssocArray { key: right_key, value: right_value },
        ) => PhpType::AssocArray {
            key: Box::new(merge_ir_assoc_value_type(
                (**left_key).clone(),
                (**right_key).clone(),
            )),
            value: Box::new(merge_ir_assoc_value_type(
                (**left_value).clone(),
                (**right_value).clone(),
            )),
        },
        // `int` and `bool` share a register representation but not a PHP type: letting one side
        // win relabels the other (`$c ? 42 : false` dumped `bool(true)`, `$c ? true : 42` dumped
        // `int(1)`). Like every other pair of distinct scalars they merge into a boxed `mixed`.
        _ => PhpType::Mixed,
    }
}

/// Removes the null sentinel type from nullable unions after a successful `??` value branch.
pub(super) fn strip_void_from_union(php_type: PhpType) -> PhpType {
    let PhpType::Union(members) = php_type else {
        return php_type;
    };
    let mut non_void = members
        .into_iter()
        .filter(|member| !matches!(member, PhpType::Void))
        .collect::<Vec<_>>();
    if non_void.is_empty() {
        PhpType::Void
    } else if non_void.len() == 1 {
        non_void.remove(0)
    } else {
        PhpType::Union(non_void)
    }
}

/// Lowers `expr ?: default`, preserving single evaluation of the first expression.
pub(super) fn lower_short_ternary(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let condition_span = value.span;
    let value = lower_expr(ctx, value);
    let cond = ctx.truthy(value, Some(condition_span));
    let split_initialized = ctx.initialized_slots_snapshot();
    let value_block = ctx
        .builder
        .create_named_block("short_ternary.value", Vec::new());
    let default_block = ctx
        .builder
        .create_named_block("short_ternary.default", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("short_ternary.merge", Vec::new());
    let join = crate::ir_lower::stmt::LazyBranchJoin::begin(ctx, merge);
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: value_block,
        then_args: Vec::new(),
        else_target: default_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(value_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    join.enter_edge(ctx);
    // A null value is never truthy, so this arm's null member does not join the merge.
    let value_branch = park_pending_branch(ctx, value, None).map(PendingBranchValue::excluding_null);

    ctx.builder.position_at_end(default_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    join.enter_edge(ctx);
    let default_value = lower_expr(ctx, default);
    let default_branch = park_pending_branch(ctx, default_value, Some(value));

    lower_two_way_pending_merge(ctx, value_branch, default_branch, merge, &split_initialized, Some(join), expr.span)
}

/// Releases a lowered value that a lazy branch tested but did not forward.
pub(super) fn release_discarded_branch_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_needs_release_after_retaining_store(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

