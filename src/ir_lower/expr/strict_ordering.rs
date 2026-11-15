//! Purpose:
//! Lowers noncoercive scalar ordering with bytewise string semantics.
//!
//! Called from:
//! - Comparison expression lowering after both source operands have been evaluated.
//!
//! Key details:
//! - Ordinary EIR guards, scalar operations and branch parameters cover every target.
//! - Owned operands remain published during catchable domain failures.
//! - String storage extraction follows the original-kind guard and cannot call Stringable hooks.

use super::*;

/// Guards original kinds and dispatches only the string ordering that differs from PHP.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, lhs: LoweredValue, operator: &BinOp, rhs: LoweredValue,
    left: &Expr, right: &Expr, expression: &Expr,
) -> LoweredValue {
    guard(ctx, lhs, rhs, expression.span);
    if lhs.ir_type == IrType::Str && rhs.ir_type == IrType::Str {
        return strings(ctx, lhs, operator, rhs, expression.span);
    }
    let result_type = if *operator == BinOp::Spaceship { PhpType::Int } else { PhpType::Bool };
    let string_block = ctx.builder.create_named_block("strict.order.strings", Vec::new());
    let scalar_block = ctx.builder.create_named_block("strict.order.scalars", Vec::new());
    let done = ctx.builder.create_named_block("strict.order.done",
        vec![(IrType::I64, result_type.clone())]);
    let is_string = crate::ir_lower::semantic_guards::matches_any_kind(ctx, lhs,
        &[crate::ir::PhpTypePredicate::String], Some(expression.span));
    ctx.builder.terminate(Terminator::CondBr { cond: is_string.value,
        then_target: string_block, then_args: Vec::new(), else_target: scalar_block, else_args: Vec::new() });
    ctx.builder.position_at_end(string_block);
    let value = strings(ctx, lhs, operator, rhs, expression.span);
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![value.value] });
    ctx.builder.position_at_end(scalar_block);
    let value = super::comparisons::lower_compare_values(ctx, lhs, operator, rhs,
        left, right, expression, true);
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![value.value] });
    ctx.builder.position_at_end(done);
    LoweredValue { value: ctx.builder.block_param(done, 0), ir_type: IrType::I64 }
}

/// Accepts matching scalar tags while retaining every original owner through a failing branch.
fn guard(ctx: &mut LoweringContext<'_, '_>, lhs: LoweredValue, rhs: LoweredValue, span: Span) {
    use crate::ir::PhpTypePredicate;
    if crate::semantics::comparisons::plan(&ctx.builder.value_php_type(lhs.value),
        &ctx.builder.value_php_type(rhs.value)) == elephc_semantics::comparisons::OrderingPlan::Exact
    { return; }
    let pins = pin_in_flight_owners(ctx, &[lhs.value, rhs.value], span);
    let mut accepted = emit_bool_literal(ctx, false, Some(span));
    for predicate in [PhpTypePredicate::Int, PhpTypePredicate::Float,
        PhpTypePredicate::Bool, PhpTypePredicate::String]
    {
        let left = crate::ir_lower::semantic_guards::matches_any_kind(ctx, lhs, &[predicate], Some(span));
        let right = crate::ir_lower::semantic_guards::matches_any_kind(ctx, rhs, &[predicate], Some(span));
        let pair = boolean_pair(ctx, Op::IBitAnd, left, right, span);
        accepted = boolean_pair(ctx, Op::IBitOr, accepted, pair, span);
    }
    let left = ctx.emit_value(Op::IsNull, vec![lhs.value], None, PhpType::Bool, Op::IsNull.default_effects(), Some(span));
    let right = ctx.emit_value(Op::IsNull, vec![rhs.value], None, PhpType::Bool, Op::IsNull.default_effects(), Some(span));
    let null_pair = boolean_pair(ctx, Op::IBitAnd, left, right, span);
    accepted = boolean_pair(ctx, Op::IBitOr, accepted, null_pair, span);
    crate::ir_lower::semantic_guards::require(ctx, accepted, Op::ThrowTypeError,
        elephc_semantics::comparisons::ORDERING_MESSAGE, Some(span));
    unpin_in_flight_owners(ctx, pins, span);
}

/// Combines noncoercive predicates using their already normalized boolean storage.
fn boolean_pair(ctx: &mut LoweringContext<'_, '_>, operator: Op,
    left: LoweredValue, right: LoweredValue, span: Span) -> LoweredValue {
    ctx.emit_value(operator, vec![left.value, right.value], None, PhpType::Bool,
        operator.default_effects(), Some(span))
}

/// Orders bytes and retires extracted string copies separately from their original operands.
fn strings(ctx: &mut LoweringContext<'_, '_>, lhs: LoweredValue,
    operator: &BinOp, rhs: LoweredValue, span: Span) -> LoweredValue {
    let left = string_storage(ctx, lhs, span);
    let right = string_storage(ctx, rhs, span);
    let compare = |ctx: &mut LoweringContext<'_, '_>, predicate| {
        ctx.emit_value(Op::StrCmp, vec![left.value, right.value],
            Some(Immediate::CmpPredicate(predicate)), PhpType::Bool, Op::StrCmp.default_effects(), Some(span))
    };
    let result = if *operator == BinOp::Spaceship {
        let greater = compare(ctx, CmpPredicate::Sgt);
        let less = compare(ctx, CmpPredicate::Slt);
        ctx.emit_value(Op::ISub, vec![greater.value, less.value], None,
            PhpType::Int, Op::ISub.default_effects(), Some(span))
    } else { compare(ctx, cmp_predicate(operator)) };
    if left.value != lhs.value { crate::ir_lower::ownership::release_if_owned(ctx, left, Some(span)); }
    if right.value != rhs.value { crate::ir_lower::ownership::release_if_owned(ctx, right, Some(span)); }
    release_binary_operand_temporary(ctx, lhs, span);
    if rhs.value != lhs.value { release_binary_operand_temporary(ctx, rhs, span); }
    result
}

/// Extracts an accepted runtime string without consuming the original boxed operand lease.
fn string_storage(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    if value.ir_type == IrType::Str { return value; }
    ctx.emit_owned_value(Op::Cast, vec![value.value], Some(Immediate::CastTarget(IrType::Str)),
        PhpType::Str, Op::Cast.default_effects(), Some(span))
}
