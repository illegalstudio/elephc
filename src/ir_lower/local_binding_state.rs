//! Purpose:
//! Records source binding presence independently of native slot allocation and representation.
//!
//! Called from:
//! - Lowering context initialization, alias binding and binding retirement.
//!
//! Key details:
//! - Eval synchronization and scope enumeration select frames needing these state writes.
//! - Representation-only stores preserve presence and cannot invent an imported binding.

use super::context::LoweringContext;
use crate::{ir::{Immediate, LocalKind, LocalSlotId, Op}, types::PhpType};

/// Publishes an actual binding event after storage initialization or before binding retirement.
pub(super) fn emit(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId, initialized: bool) {
    if !ctx.builder.function().flags.tracks_local_binding_state || ctx.binding_state_suppressed
        || ctx.builder.local_kind(slot) != LocalKind::PhpLocal
        || ctx.builder.function().locals[slot.as_raw() as usize].name.as_deref()
            .is_none_or(crate::names::is_generated_local_name) { return; }
    let value = ctx.emit_value(Op::ConstBool, Vec::new(), Some(Immediate::Bool(initialized)),
        PhpType::Bool, Op::ConstBool.default_effects(), None);
    ctx.emit_void(Op::SetLocalBindingState, vec![value.value], Some(Immediate::LocalSlot(slot)),
        Op::SetLocalBindingState.default_effects(), None);
}

/// Prepares only an initialized native origin, retaining eval origins and MAY alias facts at its join.
pub(super) fn when_native_bound(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId,
    span: crate::span::Span, body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    with_origin_condition(ctx, slot, span, Op::GetNativeBindingState, true, body);
}

/// Retains a native source guard unless eval has established a replacement binding of its own.
pub(super) fn unless_eval_bound(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId,
    span: crate::span::Span, body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    with_origin_condition(ctx, slot, span, Op::GetEvalBindingState, false, body);
}

/// Emits one provenance-dependent effect without claiming initialization on the other predecessor.
fn with_origin_condition(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId,
    span: crate::span::Span, predicate: Op, take_when_true: bool,
    body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let present = ctx.emit_value(predicate, Vec::new(),
        Some(Immediate::LocalSlot(slot)), PhpType::Bool, predicate.default_effects(), Some(span));
    with_condition(ctx, present, take_when_true, body);
}

/// Establishes an independent producer only on paths without an existing native or eval binding.
pub(super) fn when_unbound(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId,
    span: crate::span::Span, body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let present = binding_is_present(ctx, slot, span);
    with_condition(ctx, present, false, body);
}

/// Runs an observable scope read only when a native or eval binding is currently present.
pub(super) fn when_bound(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId,
    span: crate::span::Span, body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let present = binding_is_present(ctx, slot, span);
    with_condition(ctx, present, true, body);
}

/// Combines the existing provenance bits without conflating an allocated slot with a binding.
pub(super) fn binding_is_present(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId,
    span: crate::span::Span,
) -> super::context::LoweredValue {
    let native = ctx.emit_value(Op::GetNativeBindingState, Vec::new(),
        Some(Immediate::LocalSlot(slot)), PhpType::Bool, Op::GetNativeBindingState.default_effects(), Some(span));
    let eval = ctx.emit_value(Op::GetEvalBindingState, Vec::new(),
        Some(Immediate::LocalSlot(slot)), PhpType::Bool, Op::GetEvalBindingState.default_effects(), Some(span));
    ctx.emit_value(Op::IBitOr, vec![native.value, eval.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(span))
}

/// Shares the guarded CFG and initialization join for single-bit and combined presence tests.
fn with_condition(ctx: &mut LoweringContext<'_, '_>, present: super::context::LoweredValue,
    take_when_true: bool,
    body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let initialized = ctx.initialized_slots_snapshot();
    let bound = ctx.builder.create_named_block("eval.binding.selected", Vec::new());
    let done = ctx.builder.create_named_block("eval.binding.join", Vec::new());
    let (yes, no) = if take_when_true { (bound, done) } else { (done, bound) };
    ctx.builder.terminate(crate::ir::Terminator::CondBr { cond: present.value,
        then_target: yes, then_args: Vec::new(), else_target: no, else_args: Vec::new() });
    ctx.builder.position_at_end(bound);
    body(ctx);
    ctx.builder.terminate(crate::ir::Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(done);
    // Existing EIR reference analysis computes the raw/ref-cell representation at the join.
    ctx.restore_initialized_slots(initialized);
}
