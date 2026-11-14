//! Purpose:
//! Lowers explicit checked numeric casts with an original-value owner and typed runtime target.
//!
//! Called from:
//! - Scalar cast lowering and the fallback behind reviewed native object adapters.
//!
//! Key details:
//! - Operands are borrowed by runtime helpers and rooted across catchable failures.

use super::*;
use elephc_semantics::{numeric_cast::NumericCastTarget, StrictFeature};

/// Selects checked numeric behavior only for explicit user operations.
pub(super) fn active(ctx: &LoweringContext<'_, '_>, target: &CastType) -> bool {
    ctx.source_mode != crate::source::SourceMode::Internal
        && ctx.semantic_profile.contains(StrictFeature::Casts)
        && matches!(target, CastType::Int | CastType::Float)
}

/// Borrows an evaluated value, roots its boxed form and returns an exact scalar result.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, target: &CastType,
    input: LoweredValue, expr: &Expr) -> LoweredValue {
    let original = pin_in_flight_owners(ctx, &[input.value], expr.span);
    let operand = crate::ir_lower::numeric_operands::box_value(ctx, input, expr.span);
    let boxed = if operand.value != input.value { pin_in_flight_owners(ctx, &[operand.value], expr.span) } else { Vec::new() };
    let target = if *target == CastType::Int { NumericCastTarget::Int } else { NumericCastTarget::Float };
    let ty = match target { NumericCastTarget::Int => PhpType::Int, NumericCastTarget::Float => PhpType::Float };
    let result = ctx.emit_value(Op::RuntimeCall, vec![operand.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CheckedNumericCast(target))),
        ty, Effects::READS_HEAP | Effects::ALLOC_HEAP | Effects::MAY_THROW | Effects::MAY_FATAL, Some(expr.span));
    unpin_in_flight_owners(ctx, boxed, expr.span);
    if operand.value != input.value { release_binary_operand_temporary(ctx, operand, expr.span); }
    unpin_in_flight_owners(ctx, original, expr.span);
    result
}
