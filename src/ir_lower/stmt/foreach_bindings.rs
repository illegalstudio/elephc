//! Purpose:
//! Fetches foreach operands before publishing their value and key bindings.
//!
//! Called from:
//! - `typed_foreach::lower_foreach` at the start of each iteration body.
//!
//! Key details:
//! - Iterator callbacks observe the old bindings; publication is value before key.
//! - Fetched values and displaced bindings remain visible to exceptional cleanup.
//! - Fetched roots retire before displaced owners, with the owner stack kept in order.

use super::*;
use super::typed_foreach::{foreach_ref_value_type, foreach_value_type, lower_foreach_current_value};
use crate::ir_lower::expr::{
    register_owned_call_operand, retire_owned_call_operand, root_value_for_later_use,
    unregister_owned_call_operand,
};

/// Evaluates current and key before either PHP target is assigned.
pub(super) fn bind_current(
    ctx: &mut LoweringContext<'_, '_>, iterator: LoweredValue, source: &PhpType,
    key_name: Option<&str>, value_name: &str, by_ref: bool, span: Span,
) {
    let value_type = foreach_value_type(source);
    let value = (!by_ref).then(|| {
        let value = lower_foreach_current_value(ctx, iterator, source, &value_type, span);
        if key_name.is_some() { root_value_for_later_use(ctx, value, span) }
        else { (value, None) }
    });
    // By-reference sources are arrays, whose current reference needs no user callback.
    // Capture the key before replacing the value, since its destructor can mutate the array.
    let key = key_name.map(|_| {
        let value = ctx.emit_value(Op::IterCurrentKey, vec![iterator.value], None,
            PhpType::Mixed, Op::IterCurrentKey.default_effects(), Some(span));
        root_value_for_later_use(ctx, value, span)
    });
    let key_owner = key.as_ref().and_then(|(_, owner)| *owner);
    let previous = root_displaced_binding(ctx, value_name, span);
    if by_ref {
        let slot = ctx.declare_local(value_name, foreach_ref_value_type(source));
        ctx.release_ref_cell_owner(value_name, Some(span));
        ctx.emit_void(Op::IterCurrentValueRef, vec![iterator.value],
            Some(Immediate::LocalSlot(slot)), Op::IterCurrentValueRef.default_effects(), Some(span));
        ctx.mark_ref_bound_local(value_name);
        ctx.mark_local_initialized(value_name);
    } else if let Some((value, owner)) = value {
        // The store can consume an owning operand, including a boxed-to-reference conversion.
        // Give it its own lease so the published root stays valid through that instruction.
        let stored = if owner.is_some() {
            crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span))
        } else { value };
        ctx.store_local(value_name, stored, value_type, Some(span));
        retire_fetched(ctx, owner, &[key_owner, previous], span);
    }
    if let Some(owner) = previous { retire_owned_call_operand(ctx, owner, span); }
    if let (Some(name), Some((key, owner))) = (key_name, key) {
        let previous = root_displaced_binding(ctx, name, span);
        let stored = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, key, Some(span));
        ctx.store_local(name, stored, PhpType::Mixed, Some(span));
        retire_fetched(ctx, owner, &[previous], span);
        if let Some(owner) = previous { retire_owned_call_operand(ctx, owner, span); }
    }
}

/// Holds the actual old payload until its replacement has been published.
fn root_displaced_binding(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span,
) -> Option<LocalSlotId> {
    let previous = ctx.load_local(name, Some(span));
    root_value_for_later_use(ctx, previous, span).1
}

/// Retires a fetched lease while keeping newer live owners registered during its release.
fn retire_fetched(
    ctx: &mut LoweringContext<'_, '_>, owner: Option<LocalSlotId>,
    protected: &[Option<LocalSlotId>], span: Span,
) {
    let Some(owner) = owner else { return; };
    for slot in protected.iter().rev().flatten() {
        unregister_owned_call_operand(ctx, *slot, span);
    }
    unregister_owned_call_operand(ctx, owner, span);
    for slot in protected.iter().flatten() {
        register_owned_call_operand(ctx, *slot, span);
    }
    // Re-publish every protected record before a release can invoke a throwing destructor.
    ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(owner)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
}
