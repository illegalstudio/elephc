//! Purpose:
//! Guards original key kinds only when an evaluated receiver is a PHP array.
//!
//! Called from:
//! - Array read and literal-construction lowering.
//!
//! Key details:
//! - Mixed receivers retain string and ArrayAccess dispatch; operands are never converted.

use super::{context::{LoweredValue, LoweringContext}, conversion_boundaries, semantic_guards};
use crate::{ir::{IrHeapKind, IrType, PhpTypePredicate, Terminator}, span::Span};
use elephc_semantics::StrictFeature;

/// Keeps an evaluated receiver alive across a catchable key check and returns the original key.
pub(super) fn for_receiver(ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue,
    key: LoweredValue, span: Span) -> LoweredValue {
    if !ctx.semantic_profile.contains(StrictFeature::ArrayKeys) { return key; }
    let dynamic = match receiver.ir_type {
        IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash) => false,
        IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union) => true,
        _ => return key,
    };
    let pins = super::expr::call_operand_owners::pin_in_flight_owners(ctx, &[receiver.value], span);
    if dynamic {
        let is_array = semantic_guards::matches_any_kind(ctx, receiver, &[PhpTypePredicate::Array], Some(span));
        let checked = ctx.builder.create_named_block("array.key.checked", Vec::new());
        let continuation = ctx.builder.create_named_block("array.key.continue", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: is_array.value,
            then_target: checked, then_args: Vec::new(), else_target: continuation, else_args: Vec::new() });
        ctx.builder.position_at_end(checked);
        conversion_boundaries::array_key(ctx, key, span);
        ctx.builder.terminate(Terminator::Br { target: continuation, args: Vec::new() });
        ctx.builder.position_at_end(continuation);
    } else {
        conversion_boundaries::array_key(ctx, key, span);
    }
    super::expr::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
    key
}
