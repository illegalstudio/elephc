//! Purpose:
//! Preserves detached string receiver copies until a selected byte has its own storage.
//!
//! Called from:
//! - Array-access lowering before index evaluation and after the completed string read.
//!
//! Key details:
//! - Final-storage replay identifies local reads that become owned Mixed-to-string conversions.
//! - Ordinary raw string offsets keep their allocation-free borrowed view.
//! - Published owners cover throwing index expressions and missing-offset warning handlers.

use crate::ir::{IrType, LocalSlotId, Op, Ownership};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::ir_lower::ownership;
use crate::span::Span;

/// Records a provisional raw read or roots a receiver that final storage requires us to own.
pub(super) fn prepare_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<LocalSlotId>) {
    if receiver.ir_type != IrType::Str
        || !matches!(ctx.builder.value_defining_op(receiver.value), Some(Op::LoadLocal | Op::LoadStaticLocal))
        || !ctx.value_needs_release_after_use(receiver)
    {
        return (receiver, None);
    }
    if !ctx.owned_string_offset_sites.contains(&span) && !ctx.value_is_owning_temporary(receiver) {
        ctx.string_offset_receivers.push((span, receiver.value));
        return (receiver, None);
    }
    let (rooted, owner) = super::root_owned_call_operand(ctx, receiver, span);
    // The hidden slot owns the root. The read borrows it until finish_read, so
    // ordinary receiver stabilization must not release the same lease again.
    let borrowed = ctx.emit_value(
        Op::Borrow,
        vec![rooted.value],
        None,
        ctx.builder.value_php_type(rooted.value),
        Op::Borrow.default_effects(),
        Some(span),
    );
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    (borrowed, owner)
}

/// Persists the byte before retiring the receiver's guarded whole-string owner.
pub(super) fn finish_read(
    ctx: &mut LoweringContext<'_, '_>,
    result: LoweredValue,
    owner: LocalSlotId,
    span: Span,
) -> LoweredValue {
    let result = ownership::acquire_if_refcounted(ctx, result, Some(span));
    super::retire_owned_call_operand(ctx, owner, span);
    result
}
