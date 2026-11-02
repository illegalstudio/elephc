//! Purpose:
//! Exposes implicit integer conversions of checked arithmetic as scalar EIR sinks.
//!
//! Called from:
//! - Typed destination and declared-local assignment lowering.
//!
//! Key details:
//! - Only closed add/sub/mul trees with concrete integer leaves are accepted.
//! - Conversion happens after the whole tree, preserving intermediate float promotion.
//! - Unknown Mixed values and reference places retain their existing boundary handling.

use super::*;

/// Lowers a call parameter's literal storage and exposes its numeric integer conversion.
pub(super) fn lower_expr_for_parameter(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    destination: &PhpType,
) -> LoweredValue {
    let value = lower_expr_for_destination(ctx, expr, destination);
    coerce_integer_arithmetic(ctx, value, destination, expr.span)
}

/// Makes a checked arithmetic tree's existing integer destination conversion explicit.
pub(crate) fn coerce_integer_arithmetic(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    destination: &PhpType,
    span: Span,
) -> LoweredValue {
    if destination != &PhpType::Int || !checked_integer_tree(ctx, value.value) {
        return value;
    }
    coerce_to_int_at_span(ctx, value, Some(span))
}

/// Requires every noninteger node to be checked arithmetic rather than a dynamic PHP value.
fn checked_integer_tree(ctx: &LoweringContext<'_, '_>, root: ValueId) -> bool {
    if ctx.builder.value_type(root) == IrType::I64 {
        return false;
    }
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(value) = pending.pop() {
        if !seen.insert(value) { continue; }
        if ctx.builder.value_type(value) == IrType::I64
            && ctx.builder.value_php_type(value) == PhpType::Int
        {
            continue;
        }
        let Some(inst) = ctx.builder.value_defining_instruction(value) else { return false; };
        let arithmetic = matches!(inst.op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)
            || (inst.op == Op::MixedNumericBinop && matches!(inst.immediate,
                Some(Immediate::MixedNumericOp(MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))));
        if !arithmetic || inst.operands.len() != 2 { return false; }
        pending.extend(inst.operands.iter().copied());
    }
    true
}
