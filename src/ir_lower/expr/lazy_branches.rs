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
    let value_expr = value;
    let value = lower_null_coalesce_value(ctx, value_expr);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    // An all-null user call on the value side merges as the checker types it, as `?:` does
    // (`null_only_call_merge_type`), when the lowered value is that call's `array<mixed>` storage;
    // it is converted into the temp. A value loaded back through a slot that keeps its own type
    // (a global alias, a static or reference-bound local) keeps that type, as on main.
    let result_type = match super::merge_temps::null_only_call_merge_type(ctx, value_expr) {
        Some(checker)
            if ctx.builder.value_php_type(value.value).codegen_repr()
                == crate::types::dynamic_params::null_only_container_return_storage(&checker)
                && !assigns_through_kept_type_slot(ctx, value_expr) =>
        {
            null_coalesce_merge_type(ctx, checker, default)
        }
        _ => null_coalesce_result_type(ctx, value.value, default),
    };
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let default_block = ctx
        .builder
        .create_named_block("coalesce.default", Vec::new());
    let value_block = ctx.builder.create_named_block("coalesce.value", Vec::new());
    let merge = ctx.builder.create_named_block("coalesce.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: default_block,
        then_args: Vec::new(),
        else_target: value_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(default_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_expr_into_temp(ctx, &temp_name, result_type.clone(), default, expr.span);
    release_discarded_branch_value(ctx, value, expr.span);
    let default_reachable = !ctx.builder.insertion_block_is_terminated();
    let default_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(value_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_value_into_temp(ctx, &temp_name, result_type, value, expr.span);
    let value_reachable = !ctx.builder.insertion_block_is_terminated();
    let value_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &split_initialized,
        default_initialized,
        default_reachable,
        value_initialized,
        value_reachable,
    ));
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Returns true when the value side of `??` assigns through a local that keeps its own slot type
/// (static or reference-bound), so the reloaded value carries that type rather than the call's.
///
/// Walks the chain the way `null_only_call_merge_type` does, so a non-local target whose value is
/// bound to a prelude temp (`$d[0] = ($s = f())`) still reaches the inner slot.
fn assigns_through_kept_type_slot(ctx: &LoweringContext<'_, '_>, mut expr: &Expr) -> bool {
    loop {
        match &expr.kind {
            ExprKind::ErrorSuppress(inner) => expr = inner,
            ExprKind::Assignment { target, value, result_target, prelude, .. } => {
                if let ExprKind::Variable(name) = &target.kind {
                    if ctx.is_ref_bound_local(name)
                        || matches!(
                            ctx.local_kinds.get(name.as_str()),
                            Some(crate::ir::LocalKind::StaticLocal)
                        )
                    {
                        return true;
                    }
                }
                expr = super::merge_temps::assignment_result_expr(
                    value,
                    result_target.as_deref(),
                    prelude,
                );
            }
            _ => return false,
        }
    }
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
    // An overloaded name on a class with `__isset`: php asks `__isset` first and reads through
    // `__get` only when it says yes, so `??` sees null without ever calling `__get` otherwise.
    // The receiver is evaluated once, before either call: `make()->name ?? $d` runs `make()` once.
    if lazy_empty_magic_property_calls(ctx, value).is_some() {
        let (object, property, nullsafe) = match &value.kind {
            ExprKind::PropertyAccess { object, property } => (object, property, false),
            ExprKind::NullsafePropertyAccess { object, property } => (object, property, true),
            _ => unreachable!("only property accesses have magic existence calls"),
        };
        let (receiver, receiver_temp) = evaluate_once(ctx, object, value);
        let rebuilt = Expr::new(
            if nullsafe {
                ExprKind::NullsafePropertyAccess { object: Box::new(receiver), property: property.clone() }
            } else {
                ExprKind::PropertyAccess { object: Box::new(receiver), property: property.clone() }
            },
            value.span,
        );
        let result = match lazy_empty_magic_property_calls(ctx, &rebuilt) {
            Some((exists, get)) => {
                let probe = Expr::new(
                    ExprKind::Ternary {
                        condition: Box::new(exists),
                        then_expr: Box::new(get),
                        else_expr: Box::new(Expr::new(ExprKind::Null, value.span)),
                    },
                    value.span,
                );
                lower_expr(ctx, &probe)
            }
            // The held receiver no longer types as the magic class: probe it the ordinary way.
            None => lower_null_coalesce_value(ctx, &rebuilt),
        };
        release_evaluated_once(ctx, receiver_temp, value.span);
        return result;
    }
    if let ExprKind::PropertyAccess { object, property } = &value.kind {
        // The left operand of `??` is one of php's silent probes, exactly like `isset()`: a name
        // this scope may not reach answers the DEFAULT rather than raising, and a name that was
        // never created answers it without an `Undefined property` warning. A receiver whose
        // class is only known at run time needs the runtime-name form to carry that mode.
        let needs_probe = property_probe_needs_runtime_name_form(ctx, object);
        let object = lower_member_receiver(ctx, object, value);
        if property_can_be_uninitialized(ctx, object.value, property)
            || union_hook_probe_class(ctx, object.value, property).is_some() {
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
    null_coalesce_merge_type(ctx, ctx.builder.value_php_type(value), default)
}

/// Merges the type of the value side of `??` with the materialized type of its default.
///
/// Shared by the lowering, which knows the lowered value's type, and by the merge prediction
/// of an enclosing ternary, match or array literal, which estimates it from the expression.
/// Both must agree, or the outer temp is typed from a narrower guess and casts the boxed
/// `??` result back (an int/bool pair boxed here would be read back as integer 0).
pub(super) fn null_coalesce_merge_type(
    ctx: &LoweringContext<'_, '_>,
    value_ty: PhpType,
    default: &Expr,
) -> PhpType {
    let raw_value_ty = value_ty;
    let value_ty = strip_void_from_union(raw_value_ty.clone()).codegen_repr();
    // Typed once: the pointer and buffer checks below and the ordinary merge all read it, and a
    // nested merge inside the default must not be scanned per check.
    let declared_default_ty = super::merge_temps::null_only_call_merge_type(ctx, default)
        .unwrap_or_else(|| materialized_expr_type_for_merge(ctx, default));
    // A pointer array beside a `[null]` (or another pointer array) default keeps pointer storage.
    if super::indexed_array_literals::literal_pointer_element_type(&value_ty).is_some() {
        if super::indexed_array_literals::is_null_only_literal(default) {
            if matches!(default.kind, ExprKind::ArrayLiteralAssoc(_))
                == matches!(value_ty, PhpType::AssocArray { .. })
            {
                return value_ty;
            }
        } else {
            let default_ty = declared_default_ty.codegen_repr();
            if super::indexed_array_literals::literal_pointer_element_type(&default_ty).is_some() {
                if let Some(merged) =
                    super::merge_temps::merge_pointer_containers(value_ty.clone(), default_ty)
                {
                    return merged;
                }
            }
        }
    }
    // A buffer array beside an empty literal default keeps its storage: `[]` adds no element.
    if super::indexed_array_literals::literal_raw_word_element_type(&value_ty)
        .is_some_and(|elem| matches!(elem, PhpType::Buffer(_)))
        && super::indexed_array_literals::is_empty_literal(default)
        && matches!(default.kind, ExprKind::ArrayLiteralAssoc(_))
            == matches!(value_ty, PhpType::AssocArray { .. })
    {
        return value_ty;
    }
    // A `?ptr<T>` on either side keeps the merge nullable when the other side may be null: the
    // codegen form of both (the bare pointer, or `Void` for a literal null) would drop that.
    // `$p ?? null` with a non-null `ptr<T>` keeps the plain pointer, its default being dead.
    let value_nullable_pointer = crate::types::is_nullable_pointer_type(&raw_value_ty);
    let default_nullable_pointer = crate::types::is_nullable_pointer_type(&declared_default_ty);
    let value_is_null = matches!(raw_value_ty, PhpType::Void | PhpType::Never);
    let default_is_null = matches!(declared_default_ty, PhpType::Void | PhpType::Never);
    // A plain `ptr` left side is never null for `??`, so it keeps its own pointer type.
    if (value_nullable_pointer && (default_nullable_pointer || default_is_null))
        || (value_is_null && default_nullable_pointer)
    {
        return nullable_aware_branch_merge_type(&value_ty, &declared_default_ty);
    }
    let default_ty = declared_default_ty.codegen_repr();
    // `$o ?? null` can still be null. An object or callable slot has no null of its own, so a
    // default that may be null keeps the merge nullable (boxed) instead of letting the non-null
    // side's slot type win and then reading the stored null back as an object.
    if matches!(value_ty, PhpType::Object(_) | PhpType::Callable) && php_type_allows_null(&default_ty) {
        return nullable_aware_branch_merge_type(&value_ty, &default_ty);
    }
    materialized_merge_type(&value_ty, &default_ty)
}

/// Merges the materialized types of two branches whose values are both known.
///
/// Unlike the syntactic fallback widening, an int and a bool stay apart in a boxed `Mixed`
/// temp: storing an int in a bool slot (or the reverse) changes what PHP prints, so
/// `0 ?? false` must echo `0` and `false` must echo nothing.
pub(super) fn materialized_merge_type(left: &PhpType, right: &PhpType) -> PhpType {
    let left = left.codegen_repr();
    let right = right.codegen_repr();
    if matches!(
        (&left, &right),
        (PhpType::Int, PhpType::Bool | PhpType::False) | (PhpType::Bool | PhpType::False, PhpType::Int)
    ) {
        return PhpType::Mixed;
    }
    wider_type_for_merge(&left, &right)
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
        (
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never,
        ) => right.clone(),
        // A packed record address and a `ptr<T>` are the same raw pointer word. Boxing them as
        // `Mixed` would tag the address like an integer, so field reads through the merge would
        // miss the record. An untyped side (`ptr_null()`) adopts the other side's class; two
        // different classes merge to an untyped pointer.
        (
            PhpType::Pointer(_) | PhpType::Packed(_),
            PhpType::Pointer(_) | PhpType::Packed(_),
        ) => {
            let class_of = |ty: &PhpType| match ty {
                PhpType::Pointer(class) => class.clone(),
                PhpType::Packed(class) => Some(class.clone()),
                _ => None,
            };
            match (class_of(&left), class_of(&right)) {
                (Some(left_class), Some(right_class)) => {
                    let same = crate::names::php_symbol_key(left_class.trim_start_matches('\\'))
                        == crate::names::php_symbol_key(right_class.trim_start_matches('\\'));
                    PhpType::Pointer(same.then_some(left_class))
                }
                (Some(class), None) | (None, Some(class)) => PhpType::Pointer(Some(class)),
                (None, None) => PhpType::Pointer(None),
            }
        }
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
    let result_type = short_ternary_merge_result_type(ctx, value, default);
    let value = lower_array_value_for_merge(ctx, value).unwrap_or_else(|| lower_expr(ctx, value));
    // A retained constant-null branch uses the raw I64 null sentinel. It is PHP-falsy,
    // while `truthy` deliberately forwards ordinary I64 conditions without converting them.
    let cond = if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Void {
        emit_bool_literal(ctx, false, Some(condition_span))
    } else {
        ctx.truthy(value, Some(condition_span))
    };
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
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
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: value_block,
        then_args: Vec::new(),
        else_target: default_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(value_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_value_into_temp(ctx, &temp_name, result_type.clone(), value, expr.span);
    let value_reachable = !ctx.builder.insertion_block_is_terminated();
    let value_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(default_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_expr_into_temp(ctx, &temp_name, result_type, default, expr.span);
    release_discarded_branch_value(ctx, value, expr.span);
    let default_reachable = !ctx.builder.insertion_block_is_terminated();
    let default_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &split_initialized,
        value_initialized,
        value_reachable,
        default_initialized,
        default_reachable,
    ));
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Releases a lowered value that a lazy branch tested but did not forward.
///
/// A branch that already ended (the `throw` default of a buffer merge) has no successor to emit
/// the release into, and its exception never reaches the merge.
pub(super) fn release_discarded_branch_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    if ctx.value_needs_release_after_retaining_store(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

