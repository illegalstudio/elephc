//! Purpose:
//! Guards original key kinds only when an evaluated receiver is a PHP array.
//!
//! Called from:
//! - Array reads, literal construction, reference binding and write lowering.
//!
//! Key details:
//! - Mixed receivers retain string and ArrayAccess dispatch; operands are never converted.

use super::{context::{LoweredValue, LoweringContext}, conversion_boundaries, semantic_guards};
use crate::{ir::{IrHeapKind, IrType, Op, PhpTypePredicate, Terminator}, span::Span, types::PhpType};
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
        conditionally_require_key(ctx, is_array, key, span);
    } else {
        conversion_boundaries::array_key(ctx, key, span);
    }
    super::expr::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
    key
}

/// Protects write operands across a key failure without duplicating an earlier pending owner.
pub(super) fn for_write(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, key: LoweredValue,
    value: LoweredValue, previous: &[super::expr::PinnedInFlightOwner], span: Span,
) {
    if !needs_check(ctx, key) { return; }
    let pins = super::expr::protect_additional_pending_operands(
        ctx, &[receiver, key, value], previous, span,
    );
    for_prepared_write(ctx, receiver, key, span);
    super::expr::unpin_in_flight_owners(ctx, pins, span);
}

/// Checks operands whose caller already published every independent exceptional owner.
pub(super) fn for_prepared_write(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, key: LoweredValue, span: Span,
) {
    if !needs_check(ctx, key) { return; }
    let key = super::ownership::borrow_value(ctx, key, Some(span));
    if matches!(receiver.ir_type, IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash) | IrType::Void) {
        conversion_boundaries::array_key(ctx, key, span);
        return;
    }
    let receiver = super::ownership::borrow_value(ctx, receiver, Some(span));
    let array = semantic_guards::matches_any_kind(ctx, receiver, &[PhpTypePredicate::Array], Some(span));
    let null = ctx.emit_value(Op::IsNull, vec![receiver.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let no = ctx.emit_value(Op::ConstBool, Vec::new(), Some(crate::ir::Immediate::Bool(false)),
        PhpType::Bool, Op::ConstBool.default_effects(), Some(span));
    let false_parent = ctx.emit_value(Op::StrictEq, vec![receiver.value, no.value], None,
        PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
    let materializable = ctx.emit_value(Op::IBitOr, vec![null.value, false_parent.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    let checked = ctx.emit_value(Op::IBitOr, vec![array.value, materializable.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    conditionally_require_key(ctx, checked, key, span);
}

/// Avoids changing permissive writes or introducing guards for a proven int/string key.
fn needs_check(ctx: &LoweringContext<'_, '_>, key: LoweredValue) -> bool {
    ctx.semantic_profile.contains(StrictFeature::ArrayKeys)
        && crate::semantics::array_keys::plan(ctx.semantic_profile,
            &ctx.builder.value_php_type(key.value)) != elephc_semantics::conversions::ConversionPlan::Identity
}

/// Validates a key only on paths that operate on an existing or materialized PHP array.
fn conditionally_require_key(ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue,
    key: LoweredValue, span: Span) {
    let checked = ctx.builder.create_named_block("array.key.checked", Vec::new());
    let continuation = ctx.builder.create_named_block("array.key.continue", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: checked, then_args: Vec::new(), else_target: continuation, else_args: Vec::new() });
    ctx.builder.position_at_end(checked);
    conversion_boundaries::array_key(ctx, key, span);
    ctx.builder.terminate(Terminator::Br { target: continuation, args: Vec::new() });
    ctx.builder.position_at_end(continuation);
}
