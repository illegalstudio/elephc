//! Purpose:
//! Array push and indexed-array local storage normalization.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Detaches a boxed local before a write can replace its array payload or modify shared cells.
/// Reference locals publish through the existing reference; value aliases keep the original box.
pub(in crate::ir_lower) fn load_array_local_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    span: Span,
) -> LoweredValue {
    let ty = ctx.local_type(name);
    let value = ctx.load_local(name, Some(span));
    if ty.codegen_repr() != PhpType::Mixed
        || ctx.append_update_cell_aliases.contains(name)
    {
        return value;
    }
    let detached = ctx.emit_owned_value(
        Op::MixedClone,
        vec![value.value],
        None,
        ty.clone(),
        Op::MixedClone.default_effects(),
        Some(span),
    );
    ctx.store_call_argument_local(name, detached, ty, Some(span));
    ctx.load_local(name, Some(span))
}

/// Returns the container type an undefined local must be auto-vivified as, if this write is its
/// first binding. PHP creates an indexed array for `$undefined[$k] = $v`, with no undefined-read
/// diagnostic.
pub(super) fn auto_vivified_array_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> Option<PhpType> {
    if ctx.is_ref_bound_local(name) {
        return None;
    }
    if let Some(slot) = ctx.local_slots.get(name).copied() {
        if ctx.slot_is_initialized(slot) {
            return None;
        }
    }
    match ctx.local_type(name).codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some(ctx.local_type(name)),
        _ => None,
    }
}

/// Builds the empty container an undefined array local starts from. It is stored only after the
/// offset and value expressions have run, matching PHP's evaluation order when they throw.
pub(super) fn empty_auto_vivified_container(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    ty: &PhpType,
    span: Span,
) -> LoweredValue {
    // The raw load is zero while the variable is still unbound; the runtime helper allocates
    // only in that case, so a loop body does not reset a container created by an earlier
    // iteration.
    let target = if matches!(ty.codegen_repr(), PhpType::AssocArray { .. }) {
        crate::ir::RuntimeCallTarget::AutoVivifyHash
    } else {
        crate::ir::RuntimeCallTarget::AutoVivifyArray
    };
    let loaded = ctx.load_local(name, Some(span));
    ctx.emit_value(
        Op::RuntimeCall,
        vec![loaded.value],
        Some(Immediate::RuntimeCall(target)),
        ty.clone(),
        crate::ir_lower::effects_lookup::runtime_effects(),
        Some(span),
    )
}

/// Stores the auto-vivified container into its local and returns the reloaded write receiver.
pub(super) fn publish_auto_vivified_container(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    value: LoweredValue,
    ty: PhpType,
    span: Span,
) -> LoweredValue {
    ctx.store_call_argument_local(name, value, ty, Some(span));
    ctx.load_local(name, Some(span))
}

/// Lowers `$array[] = value`.
pub(super) fn lower_array_push(ctx: &mut LoweringContext<'_, '_>, array: &str, value: &Expr, span: Span) {
    let receiver = Expr::new(ExprKind::Variable(array.into()), span);
    if crate::ir_lower::expr::lower_weak_map_denied_append(ctx, &receiver, value, span) { return; }
    let value = lower_expr(ctx, value);
    // PHP selects the current append target after RHS effects, including reference writes.
    // Loading here also gives the ordinary parent guards the RHS-updated storage type.
    // Append's final-storage normalization mutates a bound box directly. Other indexed
    // writers still require the detached cell published by load_array_local_for_write.
    let array_value = if let Some(ty) = auto_vivified_array_type(ctx, array) {
        let empty = empty_auto_vivified_container(ctx, array, &ty, span);
        publish_auto_vivified_container(ctx, array, empty, ty, span)
    } else if !ctx.has_local_slot(array) && !ctx.is_ref_bound_local(array) {
        // A write whose first-use type the checker did not retain still needs a real local.
        // PHP auto-vivifies a missing variable on `$a[] = ...`, so start from a generic boxed
        // array; the checker admitted the write.
        let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
        ctx.declare_local(array, array_ty.clone());
        let empty = ctx.emit_value(
            Op::ArrayNew,
            Vec::new(),
            Some(crate::ir::Immediate::Capacity(0)),
            array_ty.clone(),
            Op::ArrayNew.default_effects(),
            Some(span),
        );
        ctx.store_local(array, empty, array_ty, Some(span));
        ctx.load_local(array, Some(span))
    } else if ctx.is_ref_bound_local(array) {
        ctx.load_local(array, Some(span))
    } else {
        load_array_local_for_write(ctx, array, span)
    };
    let array_ty = ctx.builder.value_php_type(array_value.value);
    let value = ctx.retype_value_for_container_element(&array_ty, value, Some(span));
    let pins = crate::ir_lower::expr::pin_in_flight_owners(
        ctx,
        &[array_value.value, value.value],
        span,
    );
    if !super::nested_write_guards::guard_array_push_parent(
        ctx,
        array_value,
        value,
        &pins,
        span,
    ) {
        return;
    }
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    let op = if array_value.ir_type == IrType::Heap(crate::ir::IrHeapKind::Array) {
        Op::ArrayPush
    } else if array_value.ir_type == IrType::Heap(crate::ir::IrHeapKind::Hash) {
        Op::HashAppend
    } else if array_value.ir_type == IrType::Heap(crate::ir::IrHeapKind::Mixed) {
        Op::MixedArrayAppend
    } else {
        Op::RuntimeCall
    };
    // A hash whose stamped value type cannot hold the appended value widens its storage first,
    // like the indexed path does; PHP's untyped arrays keep the runtime value (#1951).
    let (array_value, hash_storeback) = if op == Op::HashAppend {
        prepare_hash_local_write(ctx, array, array_value, value, span)
    } else {
        (array_value, None)
    };
    if op == Op::ArrayPush {
        let (array_value, updated_ty, needs_storeback) =
            if ref_bound_mixed_indexed_array_write(ctx, array, value) {
                (array_value, Some(ctx.local_type(array)), true)
            } else {
                prepare_indexed_array_local_write(ctx, array_value, value, span)
            };
        ctx.emit_void(
            op,
            vec![array_value.value, value.value],
            None,
            op.default_effects(),
            Some(span),
        );
        let elem_ty = indexed_array_write_element_type(ctx, array_value, updated_ty.as_ref());
        finish_indexed_array_local_write(
            ctx,
            array,
            array_value,
            updated_ty,
            needs_storeback,
            span,
        );
        release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, span);
        return;
    }
    ctx.emit_void(
        op,
        vec![array_value.value, value.value],
        None,
        op.default_effects(),
        Some(span),
    );
    finish_hash_local_write(ctx, array, array_value, hash_storeback, span);
    release_persisted_string_operand(ctx, value, span);
}

/// Prepares an indexed-array local for an offset assignment.
pub(super) fn prepare_indexed_array_local_set(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<PhpType>, bool) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    if indexed_array_refcounted_set_needs_mixed_conversion(&current_ty, &value_ty) {
        let updated_ty = PhpType::Array(Box::new(PhpType::Mixed));
        let converted = ctx.emit_value(
            Op::ArrayToMixed,
            vec![array_value.value],
            None,
            updated_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            Some(span),
        );
        return (converted, Some(updated_ty), true);
    }
    prepare_indexed_array_local_write(ctx, array_value, value, span)
}

/// Coerces miss-capable scalar reads before writing them into a concrete indexed-array slot.
pub(super) fn coerce_indexed_array_set_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_ty: &PhpType,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match array_ty.codegen_repr() {
        PhpType::Array(elem_ty)
            if elem_ty.codegen_repr() == PhpType::Int
                && matches!(
                    ctx.builder.value_php_type(value.value).codegen_repr(),
                    PhpType::Mixed | PhpType::TaggedScalar | PhpType::Union(_)
                ) =>
        {
            coerce_to_int(ctx, value, span)
        }
        _ => value,
    }
}

/// Returns true when a refcounted indexed-array assignment should use Mixed slots.
pub(super) fn indexed_array_refcounted_set_needs_mixed_conversion(
    current_ty: &PhpType,
    value_ty: &PhpType,
) -> bool {
    let PhpType::Array(elem_ty) = current_ty.codegen_repr() else {
        return false;
    };
    let elem_ty = elem_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    elem_ty != value_ty
        && elem_ty != PhpType::Mixed
        && elem_ty.is_refcounted()
        && value_ty.is_refcounted()
}

/// Converts typed indexed arrays to Mixed when a local write would make them heterogeneous.
pub(in crate::ir_lower) fn prepare_indexed_array_local_write(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<PhpType>, bool) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    let Some(updated_ty) = indexed_array_write_updated_type(current_ty.clone(), value_ty) else {
        return (array_value, None, false);
    };
    if !indexed_array_write_needs_mixed_conversion(&current_ty, &updated_ty) {
        return (array_value, Some(updated_ty), false);
    }
    let converted = ctx.emit_value(
        Op::ArrayToMixed,
        vec![array_value.value],
        None,
        updated_ty.clone(),
        Op::ArrayToMixed.default_effects(),
        Some(span),
    );
    (converted, Some(updated_ty), true)
}

/// Returns the widened storage when a stamped hash value type cannot hold `value_ty`.
///
/// PHP's untyped arrays keep the runtime value, so a value the stamp cannot represent widens
/// the storage to `Mixed` values — the hash analogue of `indexed_array_write_updated_type`.
fn assoc_array_write_updated_type(storage: &PhpType, value_ty: &PhpType) -> Option<PhpType> {
    let PhpType::AssocArray { key, value } = storage.codegen_repr() else {
        return None;
    };
    if value.codegen_repr() == PhpType::Mixed || value.codegen_repr() == value_ty.codegen_repr() {
        return None;
    }
    Some(PhpType::AssocArray {
        key,
        value: Box::new(PhpType::Mixed),
    })
}

/// Converts a typed hash to Mixed values when a local write would make it heterogeneous.
pub(in crate::ir_lower) fn prepare_hash_local_write(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<PhpType>) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    let Some(updated_ty) = assoc_array_write_updated_type(&current_ty, &value_ty) else {
        return (array_value, None);
    };
    // `HashToMixed` consumes its input owner and a reference-bound local's load is borrowed,
    // so acquire the shared cell first (mirrors the typed-foreach conversion).
    let array_value = if ctx.is_ref_bound_local(array) {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, array_value, Some(span))
    } else {
        array_value
    };
    let converted = ctx.emit_value(
        Op::HashToMixed,
        vec![array_value.value],
        None,
        updated_ty.clone(),
        Op::HashToMixed.default_effects(),
        Some(span),
    );
    (converted, Some(updated_ty))
}

/// Updates local type facts and emits explicit storeback for a converted hash write.
pub(in crate::ir_lower) fn finish_hash_local_write(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    updated_ty: Option<PhpType>,
    span: Span,
) {
    if let Some(updated_ty) = updated_ty {
        ctx.store_mutated_local(array, array_value, updated_ty, Some(span));
    }
}

/// Updates local type facts and emits explicit storeback for converted array writes.
pub(in crate::ir_lower) fn finish_indexed_array_local_write(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    updated_ty: Option<PhpType>,
    needs_storeback: bool,
    span: Span,
) {
    let Some(updated_ty) = updated_ty else {
        return;
    };
    if needs_storeback {
        ctx.store_mutated_local(array, array_value, updated_ty, Some(span));
    } else {
        ctx.set_local_type(array, updated_ty);
    }
}

/// Returns true when a ref-bound indexed array should keep its caller-visible element type.
pub(in crate::ir_lower) fn ref_bound_mixed_indexed_array_write(
    ctx: &LoweringContext<'_, '_>,
    array: &str,
    value: LoweredValue,
) -> bool {
    ctx.is_ref_bound_local(array)
        && matches!(
            ctx.builder.value_php_type(value.value).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
}

/// Returns the refined array type after writing a value into an indexed array.
pub(super) fn indexed_array_write_updated_type(current_ty: PhpType, value_ty: PhpType) -> Option<PhpType> {
    match current_ty.codegen_repr() {
        PhpType::Array(elem_ty) if is_empty_indexed_array_element(elem_ty.as_ref()) => Some(
            PhpType::Array(Box::new(normalize_empty_array_write_element_type(value_ty))),
        ),
        PhpType::Array(elem_ty) if elem_ty.codegen_repr() == PhpType::Mixed => None,
        PhpType::Array(elem_ty) => {
            let elem_ty = elem_ty.codegen_repr();
            if elem_ty == value_ty.codegen_repr() {
                return None;
            }
            // Every pointer is one raw word: a pointer with another target keeps pointer
            // storage, refined to an untyped `ptr` element instead of boxed `mixed` slots.
            if let (PhpType::Pointer(_), PhpType::Pointer(_)) = (&elem_ty, &value_ty.codegen_repr()) {
                return crate::types::merge_pointer_elements(&elem_ty, &value_ty.codegen_repr())
                    .map(|merged| PhpType::Array(Box::new(merged.codegen_repr())));
            }
            let value_ty = normalize_array_write_element_type(value_ty.codegen_repr());
            if elem_ty == value_ty {
                None
            } else {
                Some(PhpType::Array(Box::new(PhpType::Mixed)))
            }
        }
        _ => None,
    }
}

/// Returns true when an indexed-array write needs runtime conversion to Mixed slots.
pub(super) fn indexed_array_write_needs_mixed_conversion(current_ty: &PhpType, updated_ty: &PhpType) -> bool {
    let PhpType::Array(current_elem) = current_ty.codegen_repr() else {
        return false;
    };
    let PhpType::Array(updated_elem) = updated_ty.codegen_repr() else {
        return false;
    };
    updated_elem.codegen_repr() == PhpType::Mixed && current_elem.codegen_repr() != PhpType::Mixed
}

/// Returns true for the placeholder element type used by empty indexed arrays.
pub(super) fn is_empty_indexed_array_element(elem_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Never | PhpType::Void)
}

/// Preserves the first concrete value type written into an empty indexed array.
pub(super) fn normalize_empty_array_write_element_type(item_type: PhpType) -> PhpType {
    match normalize_materialized_element_type(item_type) {
        PhpType::Void => PhpType::Mixed,
        other => other,
    }
}
