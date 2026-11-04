//! Purpose:
//! Exposes implicit integer conversions of checked arithmetic as scalar EIR sinks.
//!
//! Called from:
//! - Typed destination and declared-local assignment lowering.
//! - Call argument lowering, which also exposes conversions at source-typed `float` parameters.
//!
//! Key details:
//! - Only closed add/sub/mul trees with concrete integer leaves are accepted.
//! - Conversion happens after the whole tree, preserving intermediate float promotion.
//! - Unknown Mixed values and reference places retain their existing boundary handling.

use super::*;

/// Returns whether parameter `index` carries a source-written type declaration.
///
/// Builtin registry signatures never do, so their arguments keep the operand shapes that
/// builtin-specific passes match (for example the boxed `sqrt($i + c)` range reducer).
pub(super) fn parameter_is_source_typed(sig: &crate::types::FunctionSig, index: usize) -> bool {
    sig.param_type_exprs.get(index).is_some_and(Option::is_some)
}

/// Lowers parameter storage, preserving implicit callable identity and integer destinations.
///
/// `source_typed` admits the float conversion only for user-declared `float` parameters.
pub(super) fn lower_expr_for_parameter(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    destination: &PhpType,
    source_typed: bool,
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
    if source_typed && destination == &PhpType::Float {
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
