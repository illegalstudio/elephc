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
//! - The key of a by-value loop over a statically known PHP array is fetched after the value is
//!   published and names the key variable's Mixed slot, so the backend can rewrite that slot's
//!   uniquely owned cell instead of allocating one per entry.

use super::*;
use super::typed_foreach::{foreach_ref_value_type, foreach_value_type, lower_foreach_current_value};
use crate::ir_lower::expr::{
    register_owned_call_operand, retire_owned_call_operand, root_value_for_later_use,
    unregister_owned_call_operand,
};

/// Evaluates current and key before either PHP target is assigned.
///
/// `array_source` is true when the source is statically a PHP array. Its key is then read from
/// the loop's retained snapshot without user code, so the fetch can wait until the value has
/// been published and name the key variable's slot for in-place reuse of its cell.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_current(
    ctx: &mut LoweringContext<'_, '_>, iterator: LoweredValue, source: &PhpType,
    key_name: Option<&str>, value_name: &str, by_ref: bool, array_source: bool, span: Span,
) {
    let value_type = foreach_value_type(source);
    let value = (!by_ref).then(|| {
        let value = lower_foreach_current_value(ctx, iterator, source, &value_type, span);
        if key_name.is_some() { root_value_for_later_use(ctx, value, span) }
        else { (value, None) }
    });
    // A by-value array key is fetched after the value is published, see `fetch_reused_key`.
    // One name bound as both key and value keeps the ordinary order: publishing the value
    // can move that name to another slot.
    let deferred_key = key_name
        .filter(|name| !by_ref && array_source && *name != value_name)
        .filter(|name| ctx.reusable_foreach_key_slot(name).is_some());
    // By-reference sources are arrays, whose current reference needs no user callback.
    // Capture the key before replacing the value, since its destructor can mutate the array.
    let key = key_name.filter(|_| deferred_key.is_none()).map(|_| {
        let value = ctx.emit_value(Op::IterCurrentKey, vec![iterator.value], None,
            PhpType::Mixed, Op::IterCurrentKey.default_effects(), Some(span));
        root_value_for_later_use(ctx, value, span)
    });
    let key_owner = key.as_ref().and_then(|(_, owner)| *owner);
    if by_ref {
        // The backedge already carries a cell, while the first iteration can still be raw.
        ctx.mark_ref_bound_local(value_name);
    }
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
    let key = key.or_else(|| deferred_key.map(|name| fetch_reused_key(ctx, iterator, name, span)));
    if let (Some(name), Some((key, owner))) = (key_name, key) {
        let previous = root_displaced_binding(ctx, name, span);
        let stored = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, key, Some(span));
        ctx.store_local(name, stored, PhpType::Mixed, Some(span));
        retire_fetched(ctx, owner, &[previous], span);
        if let Some(owner) = previous { retire_owned_call_operand(ctx, owner, span); }
    }
}

/// Fetches a by-value array key that may rewrite the cell its key variable still owns.
///
/// Naming the key variable's slot lets the backend reuse that cell for an integer key instead
/// of allocating one per entry; the fetch then returns the cell as an owned value and leaves
/// the slot empty, which the publication that follows treats like any displaced binding. The
/// fetch runs after the value was published so that nothing able to throw or run user code
/// (the displaced value's destructor) sits between emptying the slot and refilling it: a
/// handler in the same function always sees either the previous key or the new one. The
/// iterator reads a snapshot the loop retains, so the later fetch yields the same key.
fn fetch_reused_key(
    ctx: &mut LoweringContext<'_, '_>, iterator: LoweredValue, name: &str, span: Span,
) -> (LoweredValue, Option<LocalSlotId>) {
    // The slot is looked up at the fetch itself, so it is the one the store below replaces.
    let slot = ctx.reusable_foreach_key_slot(name).map(Immediate::LocalSlot);
    let value = ctx.emit_value(Op::IterCurrentKey, vec![iterator.value], slot, PhpType::Mixed,
        Op::IterCurrentKey.default_effects(), Some(span));
    root_value_for_later_use(ctx, value, span)
}

/// Holds the actual old payload until its replacement has been published.
fn root_displaced_binding(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span,
) -> Option<LocalSlotId> {
    let previous = ctx.load_local_for_retirement_pin(name, Some(span));
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
