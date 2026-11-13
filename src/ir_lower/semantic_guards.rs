//! Purpose:
//! Builds typed semantic guards with catchable failure and balanced temporary ownership.
//!
//! Called from:
//! - Strict operation, assignment, call, and conversion lowering.
//!
//! Key details:
//! - Predicates inspect the original value; guards never perform an implicit conversion.

use super::context::{LoweredValue, LoweringContext};
use crate::ir::{Immediate, Op, PhpTypePredicate, Terminator};
use crate::span::Span;
use crate::types::PhpType;

/// Throws a typed semantic failure when a computed predicate is false.
pub(super) fn require(
    ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue,
    failure: Op, message: &str, span: Option<Span>,
) {
    require_condition(ctx, condition, true, failure, message, span);
}

/// Throws when a failure predicate is true, allowing native overflow-branch fusion.
pub(super) fn require_false(
    ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue,
    failure: Op, message: &str, span: Option<Span>,
) {
    require_condition(ctx, condition, false, failure, message, span);
}

/// Shares failure/continuation construction without a redundant boolean negation.
fn require_condition(
    ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue, accept_when_true: bool,
    failure: Op, message: &str, span: Option<Span>,
) {
    let success = ctx.builder.create_named_block("strict.accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("strict.rejected", Vec::new());
    let (then_target, else_target) = if accept_when_true { (success, rejected) } else { (rejected, success) };
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value, then_target, then_args: Vec::new(),
        else_target, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(rejected);
    let message = ctx.intern_string(message);
    ctx.emit_void(failure, Vec::new(), Some(Immediate::Data(message)), failure.default_effects(), span);
    ctx.builder.terminate(Terminator::Unreachable);
    ctx.builder.position_at_end(success);
}

/// Requires a PHP value kind, preserving the input and its owner across a failing guard.
pub(super) fn require_kind(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    predicate: PhpTypePredicate, feature: &str, span: Option<Span>,
) -> LoweredValue {
    let pins = span.map(|span| super::expr::call_operand_owners::pin_in_flight_owners(
        ctx, &[value.value], span)).unwrap_or_default();
    let accepted = matches_any_kind(ctx, value, &[predicate], span);
    require(ctx, accepted, Op::ThrowTypeError, &format!("--{feature}: incompatible value kind"), span);
    if let Some(span) = span {
        super::expr::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
    }
    value
}

/// Combines noncoercive kind predicates while leaving exceptional ownership to the caller.
pub(super) fn matches_any_kind(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    predicates: &[PhpTypePredicate], span: Option<Span>,
) -> LoweredValue {
    let mut accepted: Option<LoweredValue> = None;
    for predicate in predicates {
        let next = ctx.emit_value(Op::TypePredicate, vec![value.value],
            Some(Immediate::TypePredicate(*predicate)), PhpType::Bool, Op::TypePredicate.default_effects(), span);
        accepted = Some(match accepted {
            Some(previous) => ctx.emit_value(Op::IBitOr, vec![previous.value, next.value], None,
                PhpType::Bool, Op::IBitOr.default_effects(), span),
            None => next,
        });
    }
    accepted.unwrap_or_else(|| ctx.emit_value(Op::ConstI64, Vec::new(), Some(Immediate::I64(0)),
        PhpType::Bool, Op::ConstI64.default_effects(), span))
}
