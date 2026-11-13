//! Purpose:
//! Guards numeric operand kinds and integer-power domains before any value conversion.
//!
//! Called from:
//! - Scalar arithmetic lowering after array and native-object overload selection.
//!
//! Key details:
//! - Both original operands remain rooted across kind and value failures.
//! - Native numeric implementation instructions remain independent of strict source contracts.

use super::{context::{LoweredValue, LoweringContext}, semantic_guards};
use crate::ir::{CmpPredicate, Immediate, IrType, Op, PhpTypePredicate, Terminator};
use crate::parser::ast::BinOp;
use crate::span::Span;
use crate::types::PhpType;
use elephc_semantics::{numeric_domains::{NumericDomain, NEGATIVE_INTEGER_POWER_MESSAGE}, StrictFeature};

/// Validates both already-evaluated operands before scalar coercion or arithmetic can run.
pub(super) fn binary(ctx: &mut LoweringContext<'_, '_>, left: LoweredValue, op: &BinOp, right: LoweredValue, span: Span) {
    let Some(rule) = crate::semantics::numeric::operand_rule(ctx.semantic_profile, ctx.source_mode, op) else { return; };
    let pins = super::expr::call_operand_owners::pin_in_flight_owners(ctx, &[left.value, right.value], span);
    let predicates: &[PhpTypePredicate] = match rule.domain {
        NumericDomain::Number => &[PhpTypePredicate::Int, PhpTypePredicate::Float],
        NumericDomain::Integer => &[PhpTypePredicate::Int],
        NumericDomain::IntegerOrString => &[PhpTypePredicate::Int, PhpTypePredicate::String],
    };
    for value in [left, right] {
        let accepted = semantic_guards::matches_any_kind(ctx, value, predicates, Some(span));
        semantic_guards::require(ctx, accepted, Op::ThrowTypeError, &rule.message(), Some(span));
    }
    if *op == BinOp::Pow && ctx.semantic_profile.contains(StrictFeature::Arithmetic) {
        require_integer_power_domain(ctx, left, right, span);
    }
    super::expr::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
}

/// Rejects a negative exponent only on the integer/integer path, retaining floating power.
fn require_integer_power_domain(ctx: &mut LoweringContext<'_, '_>, left: LoweredValue, right: LoweredValue, span: Span) {
    if left.ir_type == IrType::F64 || right.ir_type == IrType::F64 { return; }
    let left_int = semantic_guards::matches_any_kind(ctx, left, &[PhpTypePredicate::Int], Some(span));
    let right_int = semantic_guards::matches_any_kind(ctx, right, &[PhpTypePredicate::Int], Some(span));
    let both = ctx.emit_value(Op::IBitAnd, vec![left_int.value, right_int.value], None, PhpType::Bool,
        Op::IBitAnd.default_effects(), Some(span));
    let integer = ctx.builder.create_named_block("strict.integer_power", Vec::new());
    let continuation = ctx.builder.create_named_block("strict.power_domain_accepted", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: both.value, then_target: integer, then_args: Vec::new(),
        else_target: continuation, else_args: Vec::new() });
    ctx.builder.position_at_end(integer);
    let exponent = if right.ir_type == IrType::I64 { right } else {
        ctx.emit_value(Op::Cast, vec![right.value], Some(Immediate::CastTarget(IrType::I64)), PhpType::Int,
            Op::Cast.default_effects(), Some(span))
    };
    let zero = ctx.emit_value(Op::ConstI64, Vec::new(), Some(Immediate::I64(0)), PhpType::Int,
        Op::ConstI64.default_effects(), Some(span));
    let negative = ctx.emit_value(Op::ICmp, vec![exponent.value, zero.value], Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
        PhpType::Bool, Op::ICmp.default_effects(), Some(span));
    semantic_guards::require_false(ctx, negative, Op::ThrowArithmeticError, NEGATIVE_INTEGER_POWER_MESSAGE, Some(span));
    ctx.builder.terminate(Terminator::Br { target: continuation, args: Vec::new() });
    ctx.builder.position_at_end(continuation);
}
