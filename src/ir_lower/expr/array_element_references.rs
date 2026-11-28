//! Purpose:
//! Binds array element references with rooted original keys and writable receivers.
//!
//! Called from:
//! - Reference assignments and nested by-reference foreach source preparation.
//!
//! Key details:
//! - Key validation precedes normalization and retains exact string identity.
//! - Operand owners survive lookup, binding publication and catchable failures.

use super::*;
use super::property_access::prepare_addressable_ref_array_receiver;

/// Carries evaluated reference operands until the selected cell has an owning binding.
pub(super) struct ArrayReferenceOperands {
    pub(super) receiver: LoweredValue,
    pub(super) key: LoweredValue,
    key_owner: Option<crate::ir::LocalSlotId>,
    receiver_pins: Vec<PinnedInFlightOwner>,
}

impl ArrayReferenceOperands {
    /// Evaluates and roots the key before inspecting its original PHP kind.
    pub(super) fn prepare(
        ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, index: &Expr, span: Span,
    ) -> Self {
        let mut pending = [receiver];
        let ((key, key_owner), receiver_pins) = lower_with_pending_owners(
            ctx, &mut pending, span, |ctx| {
                let key = lower_expr(ctx, index);
                let (key, owner) = root_value_for_later_use(ctx, key, index.span);
                let key = match owner {
                    Some(slot) => load_published_container(ctx, slot,
                        ctx.builder.value_php_type(key.value), index.span),
                    None => key,
                };
                if ctx.semantic_profile.contains(elephc_semantics::StrictFeature::ArrayKeys) {
                    // The enclosing operand window owns receiver cleanup; the guard borrows it.
                    let borrowed = crate::ir_lower::ownership::borrow_value(ctx, receiver, Some(span));
                    crate::ir_lower::array_key_guards::for_receiver(ctx, borrowed, key, index.span);
                }
                (key, owner)
            },
        );
        Self { receiver: pending[0], key, key_owner, receiver_pins }
    }

    /// Retires keys before the earlier receiver's exceptional owner is detached.
    pub(super) fn finish(self, ctx: &mut LoweringContext<'_, '_>, span: Span) {
        if let Some(owner) = self.key_owner { retire_owned_call_operand(ctx, owner, span); }
        unpin_in_flight_owners(ctx, self.receiver_pins, span);
    }
}

/// Lowers `$target =& $arr[idx]` using the ownership represented by the container.
///
/// The addressable receiver is first represented by a local alias and normalized to hash storage.
/// Its entry then owns a managed tag-11 cell, so the new local can outlive replacement or
/// destruction of the parent. This also gives copy-on-write and growth a writable place where
/// they can publish a replacement reached through a static property or nested element.
pub(crate) fn lower_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    if instance_object_class_for_ir(ctx, array)
        .is_some_and(|class| class.eq_ignore_ascii_case("WeakMap"))
    {
        let call = Expr::new(ExprKind::MethodCall {
            object: array.clone(), method: "__elephc_reference".into(), args: vec![(**index).clone()],
        }, source.span);
        lower_ref_assign_call(ctx, target, &call, span);
        return;
    }
    let prepared_array = prepare_addressable_ref_array_receiver(ctx, array);
    let array = prepared_array.as_ref().unwrap_or(array);
    crate::ir_lower::stmt::promote_by_ref_foreach_source(ctx, array, true);
    let array_value = lower_expr(ctx, array);
    bind_array_element_reference(ctx, target, array_value, index, source, span);
    // A property read that is not addressable storage (a `mixed` property, so no synthetic
    // alias was reified above) arrives as an OWNING temporary -- `PropGet` acquires -- and
    // nothing else releases it: the binding above retains the entry's managed cell, not the
    // container, so the container's owner was simply leaked, one boxed array per
    // `$r = &$obj->m[k]`. Only the property reads are released here. A concrete container
    // loaded from a Mixed-widened local also counts as an owning temporary, but that detached
    // owner is what the backend's consuming store-back transfers, and releasing it too freed
    // the table under the local.
    if matches!(
        ctx.builder.value_defining_op(array_value.value),
        Some(Op::PropGet | Op::DynamicPropGet | Op::NullsafePropGet)
    ) && ctx.value_is_owning_temporary(array_value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, array_value, Some(span));
    }
}

/// Binds an element of an already evaluated addressable receiver without replaying its effects.
pub(super) fn bind_array_element_reference(ctx: &mut LoweringContext<'_, '_>, target: &str,
    array_value: LoweredValue, index: &Expr, source: &Expr, span: Span) {
    let operands = ArrayReferenceOperands::prepare(ctx, array_value, index, span);
    let array_value = operands.receiver;
    let container_type = ctx.builder.value_php_type(array_value.value).codegen_repr();
    let mut index_value = operands.key;
    let value_type = match container_type {
        PhpType::Array(elem_ty) => {
            if elem_ty.codegen_repr() == PhpType::Mixed {
                PhpType::Mixed
            } else {
                index_value = coerce_array_key_to_int_at_span(ctx, index_value, Some(index.span), false);
                normalize_value_php_type(*elem_ty)
            }
        }
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
        _ => array_access_result_type(ctx, array_value.value, Op::ArrayGet, source),
    };
    let cell_ptr = ctx.emit_value(
        Op::LoadArrayElemRefCell,
        vec![array_value.value, index_value.value],
        None,
        value_type.clone(),
        Op::LoadArrayElemRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
    operands.finish(ctx, span);
}

/// Binds a synthetic foreach source alias to an element after its receiver was promoted to hash.
///
/// Unlike an ordinary indexed `=&` binding, a promoted hash entry owns a managed reference cell.
/// Retaining that cell gives the synthetic origin a stable writeback address across table growth,
/// source replacement, and destruction of the enclosing container.
pub(crate) fn lower_owned_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    let array_value = lower_expr(ctx, array);
    let operands = ArrayReferenceOperands::prepare(ctx, array_value, index, span);
    let array_value = operands.receiver;
    let value_type = match ctx.builder.value_php_type(array_value.value).codegen_repr() {
        PhpType::Array(elem_ty) => normalize_value_php_type(*elem_ty),
        PhpType::AssocArray { value, .. } => normalize_value_php_type(*value),
        _ => PhpType::Mixed,
    };
    let checked = crate::ir_lower::semantic_guards::source_feature_enabled(ctx,
        elephc_semantics::StrictFeature::Indexing);
    let checked_pins = if checked {
        let mut pending = vec![array_value];
        if operands.key_owner.is_none() { pending.push(operands.key); }
        super::pending_owners::protect_additional_pending_operands(ctx, &pending, &operands.receiver_pins, span)
    } else { Vec::new() };
    let op = Op::LoadArrayElemRefCellExisting;
    let cell_ptr = ctx.emit_value(
        op,
        vec![array_value.value, operands.key.value],
        checked.then_some(Immediate::CheckedElementRead),
        value_type.clone(),
        if checked { op.checked_element_read_effects() } else { op.default_effects() },
        Some(span),
    );
    unpin_in_flight_owners(ctx, checked_pins, span);
    ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
    operands.finish(ctx, span);
}
