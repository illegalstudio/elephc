//! Purpose:
//! Publishes pending operand owners only across EIR that can unwind.
//!
//! Called from:
//! - Concatenation and array-write operand lowering.
//!
//! Key details:
//! - Lower first to inspect authoritative EIR effects, then place ownership preparation before the window.
//! - Non-throwing scalar paths introduce no ownership slots or optimizer barriers.
//! - Provisional local views use prunable guards instead of non-retaining temporary slots.

use super::*;

/// Lowers an operand window and protects earlier owners when that window can throw.
/// Warning handlers can throw too. Fresh transient strings become owned before publication;
/// both steps precede the window.
pub(crate) fn lower_with_pending_owners<'m, 'f, T>(
    ctx: &mut LoweringContext<'m, 'f>,
    operands: &mut [LoweredValue],
    span: Span,
    lower: impl FnOnce(&mut LoweringContext<'m, 'f>) -> T,
) -> (T, Vec<PinnedInFlightOwner>) {
    let anchor = ctx.builder.insertion_block().expect("pending operand insertion block");
    let position = ctx.builder.function().block(anchor).unwrap().instructions.len();
    let first_instruction = ctx.builder.function().instructions.len();
    let first_block = ctx.builder.function().blocks.len();
    let result = lower(ctx);
    let function = ctx.builder.function();
    let can_unwind = function.instructions[first_instruction..].iter()
        .any(|inst| instruction_can_unwind(function, inst))
        || matches!(function.block(anchor).unwrap().terminator, Some(crate::ir::Terminator::Throw { .. }))
        || function.blocks[first_block..].iter().any(|block| {
            matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))
        });
    if !can_unwind { return (result, Vec::new()); }
    let source = ctx.builder.insertion_block().expect("pending owner publication block");
    let start = ctx.builder.function().block(source).unwrap().instructions.len();
    for operand in operands.iter_mut() {
        if string_value_needs_persistence(ctx, *operand) {
            *operand = string_concat::persist_pending_string(ctx, *operand, span);
        }
    }
    let pins = protect_pending_operands(ctx, operands, span);
    if ctx.builder.function().block(source).unwrap().instructions.len() != start {
        ctx.builder.move_trailing_instructions(source, start, anchor, position);
    }
    (result, pins)
}

/// Publishes distinct operands in evaluation order without consuming live variable storage.
fn protect_pending_operands(
    ctx: &mut LoweringContext<'_, '_>, operands: &[LoweredValue], span: Span,
) -> Vec<PinnedInFlightOwner> {
    let mut seen = Vec::new();
    let mut records = Vec::new();
    for operand in operands {
        if seen.contains(&operand.value) { continue; }
        seen.push(operand.value);
        records.extend(ctx.guard_owned_local_views(&[*operand], span).into_iter()
            .map(|value| PinnedInFlightOwner::LocalView { value }));
        records.extend(pin_in_flight_owners(ctx, &[operand.value], span));
    }
    records
}

/// Recognizes real unwind windows while retaining canonical conservative Cast effects.
/// Integer casts of fresh checked arithmetic only inspect numeric payloads and cannot warn.
fn instruction_can_unwind(function: &crate::ir::Function, inst: &crate::ir::Instruction) -> bool {
    if inst.effects.contains(crate::ir::Effects::MAY_THROW) { return true; }
    if !inst.effects.contains(crate::ir::Effects::MAY_WARN) { return false; }
    if inst.op == Op::Cast && inst.result_php_type == PhpType::Int
        && inst.immediate == Some(Immediate::CastTarget(IrType::I64))
    {
        let producer = inst.operands.first().and_then(|value| function.value(*value))
            .and_then(|value| match value.def {
                crate::ir::ValueDef::Instruction { inst, .. } => function.instruction(inst),
                _ => None,
            });
        if producer.is_some_and(|producer| matches!(producer.op,
            Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul | Op::ICheckedPow))
        {
            return false;
        }
    }
    true
}
