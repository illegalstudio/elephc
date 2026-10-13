//! Purpose:
//! Provides small helpers for explicit EIR ownership operations during
//! AST-to-EIR lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt` and `crate::ir_lower::expr` when values cross
//!   assignment, call, and cleanup boundaries.
//!
//! Key details:
//! - Ownership and semantic value copies use explicit EIR opcodes consumed
//!   by the active backend; lifetime pins are not PHP value assignments.
//! - Explicit EIR ownership operations preserve distinct value boxes across PHP assignments.

#![allow(dead_code)]

use crate::ir::{Immediate, Op, Ownership};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::span::Span;

/// Detaches a mutable Mixed value box for a by-value assignment and consumes an owned RHS.
/// Array payloads retain COW sharing, while objects and resources preserve their PHP identity.
pub(crate) fn copy_assignment_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(value.value);
    if !matches!(php_type.codegen_repr(), crate::types::PhpType::Mixed | crate::types::PhpType::Union(_)) {
        return value;
    }
    let copied = ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, php_type,
        Op::MixedClone.default_effects(), span);
    if ctx.value_is_owning_temporary(value) { release_if_owned(ctx, value, span); }
    copied
}

/// Gives a call snapshot one independent lease without consuming the source expression's owner.
/// Mixed cells detach while container payloads keep PHP COW and object/resource identity.
pub(crate) fn copy_for_php_value_binding(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Option<Span>,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(value.value);
    if matches!(php_type.codegen_repr(), crate::types::PhpType::Mixed | crate::types::PhpType::Union(_)) {
        ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, php_type,
            Op::MixedClone.default_effects(), span)
    } else {
        acquire_if_refcounted(ctx, value, span)
    }
}

/// Emits an acquire operation when the value can carry runtime lifetime state.
pub(crate) fn acquire_if_refcounted(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(value.value);
    if Ownership::php_type_needs_lifetime_tracking(&php_type) {
        return ctx.emit_value(
            Op::Acquire,
            vec![value.value],
            None,
            php_type,
            Op::Acquire.default_effects(),
            span,
        );
    }
    value
}

/// Transfers the owner held by a directly loaded local into a terminating operation.
///
/// `throw $local` never returns to the current path, so retaining the loaded value would leave
/// the slot's original owner stranded in an abandoned frame. Clearing the slot without releasing
/// it moves that owner into the in-flight exception. Loads from widened `Mixed` slots are already
/// materialized as owned concrete values by the backend; their source box is released before the
/// slot is cleared, leaving the unboxed owned value as the transferred exception reference.
pub(crate) fn take_direct_local_load(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    let slot = {
        let inst = ctx.builder.value_defining_instruction(value.value)?;
        if inst.op != Op::LoadLocal {
            return None;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return None;
        };
        slot
    };
    // Only a concrete payload extracted from a Mixed slot has acquired its own
    // reference. A Mixed-to-Mixed load still borrows the slot's original box:
    // move that box by clearing the slot, without decref before the throw.
    if !matches!(ctx.builder.value_php_type(value.value).codegen_repr(),
        crate::types::PhpType::Mixed | crate::types::PhpType::Union(_)) && matches!(
        ctx.builder.local_php_type(slot).codegen_repr(),
        crate::types::PhpType::Mixed | crate::types::PhpType::Union(_)
    ) {
        ctx.emit_void(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::ReleaseLocalSlot.default_effects(),
            span,
        );
    }
    ctx.emit_void(
        Op::ZeroLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ZeroLocalSlot.default_effects(),
        span,
    );
    ctx.builder.set_value_ownership(value.value, Ownership::Owned);
    Some(value)
}

/// Emits an acquire that is marked as a lifetime pin: a reference taken purely so the value
/// outlives an interval, not so it can be read through the acquired result.
///
/// The `Immediate::Bool(true)` marker is what tells the paired acquire/release peephole to leave
/// this pair alone. That peephole cancels an `Acquire` whose only use is its `Release` on the
/// premise that the raised refcount in between is unobservable — true for a value nobody else
/// touches, false by construction here: the whole point of a pin is that something inside the
/// interval may drop the other owner, and cancelling the pair would hand that interval freed
/// storage (issue #580).
///
/// Returns the operand unchanged when its type carries no runtime lifetime state, so callers can
/// detect "nothing was pinned" by comparing value ids.
pub(crate) fn acquire_lifetime_pin_if_refcounted(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(value.value);
    if Ownership::php_type_needs_lifetime_tracking(&php_type) {
        return ctx.emit_value(
            Op::Acquire,
            vec![value.value],
            Some(crate::ir::Immediate::Bool(true)),
            php_type,
            Op::Acquire.default_effects(),
            span,
        );
    }
    value
}

/// Emits a type-gated release; the backend filters the value's ownership state.
pub(crate) fn release_if_owned(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Option<Span>) {
    let php_type = ctx.builder.value_php_type(value.value);
    if Ownership::php_type_needs_lifetime_tracking(&php_type)
        && !matches!(php_type, crate::types::PhpType::Void)
    {
        ctx.clear_consumed_argument_snapshot(value.value, span);
        ctx.emit_void(
            Op::Release,
            vec![value.value],
            None,
            Op::Release.default_effects(),
            span,
        );
    }
}

/// Emits an explicit cycle-collection safe point after PHP roots were updated.
pub(crate) fn collect_cycles(ctx: &mut LoweringContext<'_, '_>, span: Option<Span>) {
    ctx.emit_void(
        Op::GcCollect,
        Vec::new(),
        None,
        Op::GcCollect.default_effects(),
        span,
    );
}

/// Returns whether an ownership state means the value is potentially released by this path.
pub(crate) fn may_require_release(ownership: Ownership) -> bool {
    ownership.may_require_release()
}
