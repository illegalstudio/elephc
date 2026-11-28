//! Purpose:
//! Proves an actual EIR value non-null before an ordinary consumer uses it.
//!
//! Called from:
//! - Receiver and declared value boundaries before storage adaptation.
//!
//! Key details:
//! - Existing operand roots are reused and additional owners survive catchable failure.
//! - Null sentinels in typed heap storage still require an actual value test.

use super::{context::{LoweredValue, LoweringContext}, semantic_guards};
use crate::{ir::{IrType, Op}, span::Span, types::PhpType};
use elephc_semantics::StrictFeature;

/// Checks a declaration's null obligation without changing its independently selected conversion.
pub(super) fn require_declared_value(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    destination: &PhpType, span: Span) {
    if crate::semantics::nullability::non_null_contract(destination) {
        require_non_null(ctx, value, &[], &[], span);
    }
}

/// Checks a value without converting it or taking responsibility for earlier operand leases.
pub(super) fn require_non_null(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    pending: &[LoweredValue], previous: &[super::expr::PinnedInFlightOwner], span: Span) -> LoweredValue {
    if !semantic_guards::source_feature_enabled(ctx, StrictFeature::Nullability) { return value; }
    if matches!(value.ir_type, IrType::I64 | IrType::F64)
        && crate::semantics::nullability::null_state(&ctx.builder.value_php_type(value.value))
            == elephc_semantics::nullability::NullState::NonNull { return value; }
    let mut operands = Vec::with_capacity(pending.len() + 1);
    operands.push(value);
    operands.extend_from_slice(pending);
    let pins = super::expr::protect_additional_pending_operands(ctx, &operands, previous, span);
    let is_null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    semantic_guards::require_false(ctx, is_null, Op::ThrowTypeError,
        crate::semantics::nullability::NON_NULL_MESSAGE, Some(span));
    super::expr::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
    value
}
