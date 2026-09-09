//! Purpose:
//! Roots owned expression operands across calls that can unwind out of their caller.
//!
//! Called from:
//! - Descriptor invocation and eval call lowering.
//!
//! Key details:
//! - Frame cleanup owns each root during exceptional exit; ordinary exit retires the slot first.
//! - Borrowed local loads remain subject to final storage-aware release pruning.

use super::*;

/// Publishes a temporary owner in a frame slot and returns its stable invocation operand.
pub(super) fn root_owned_call_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>) {
    if !ctx.value_is_owning_temporary(value) && !value_is_deferred_string_local_load(ctx, value) {
        return (value, None);
    }
    let ty = ctx.builder.value_php_type(value.value);
    let name = ctx.declare_hidden_temp(ty.clone());
    // Concrete local loads can still be provisional owned unboxes until frame
    // types are finalized. Acquire explicitly, then let ownership finalization
    // prune the source release when that load turns out to be borrowed.
    let rooted = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_local(&name, rooted, ty, Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    (rooted, ctx.local_slots.get(&name).copied())
}

/// Recognizes string loads whose final slot can require a detached Mixed-to-string conversion.
fn value_is_deferred_string_local_load(
    ctx: &LoweringContext<'_, '_>,
    value: LoweredValue,
) -> bool {
    if ctx.builder.value_php_type(value.value).codegen_repr() != PhpType::Str {
        return false;
    }
    let Some(inst) = ctx.builder.value_defining_instruction(value.value) else {
        return false;
    };
    if !matches!(inst.op, Op::LoadLocal | Op::LoadStaticLocal) {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return false;
    };
    matches!(ctx.builder.local_kind(slot), crate::ir::LocalKind::PhpLocal | crate::ir::LocalKind::StaticLocal)
}

/// Clears a rooted operand before releasing it, including when its destructor throws.
pub(super) fn retire_owned_call_operand(
    ctx: &mut LoweringContext<'_, '_>,
    slot: crate::ir::LocalSlotId,
    span: Span,
) {
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(span),
    );
}
