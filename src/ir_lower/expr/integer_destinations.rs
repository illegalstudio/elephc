//! Purpose:
//! Exposes implicit integer conversions of checked arithmetic as scalar EIR sinks.
//!
//! Called from:
//! - Typed destination and declared-local assignment lowering.
//! - Call argument lowering, which also exposes `float` parameter conversions.
//!
//! Key details:
//! - Only closed add/sub/mul trees with concrete integer leaves are accepted.
//! - Conversion happens after the whole tree, preserving intermediate float promotion.
//! - Unknown Mixed values and reference places retain their existing boundary handling.

use super::*;

/// Lowers parameter storage, preserving implicit callable identity and integer destinations.
pub(super) fn lower_expr_for_parameter(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    destination: &PhpType,
) -> LoweredValue {
    if destination.codegen_repr() == PhpType::Callable {
        let source = match &expr.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => expr,
        };
        if let ExprKind::StringLiteral(name) = &source.kind {
            if let Some(target) = crate::types::param_binding::callable_target_from_name(name) {
                return lower_callable_value(ctx, &target, source);
            }
        }
    }
    let value = lower_expr_for_destination(ctx, expr, destination);
    if destination == &PhpType::Float {
        return coerce_float_arithmetic(ctx, value, expr.span);
    }
    coerce_integer_arithmetic(ctx, value, destination, expr.span)
}

/// Exposes a float parameter's conversion of a checked arithmetic tree as one scalar cast.
///
/// PHP passes an int|float argument to a `float` parameter as `(float)` of the whole result,
/// so an overflowed tree keeps its promoted double and an in-range integer converts exactly
/// once, matching the boxed call-boundary conversion.
fn coerce_float_arithmetic(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !checked_integer_tree(ctx, value.value) {
        return value;
    }
    coerce_to_float_at_span(ctx, value, Some(span))
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
