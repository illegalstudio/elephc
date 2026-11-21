//! Purpose:
//! Materializes prepared declaration values under their producer policy before consumer checks.
//!
//! Called from:
//! - The EIR expression dispatcher for bound default and constant copies.
//!
//! Key details:
//! - Integer widening preserves every non-integer alternative and evaluates the producer once.
//! - Existing owned merge slots keep boxed nullable and nominal values balanced across branches.

use super::*;
use crate::parser::ast::ProducerBinding;

/// Selects physical producer semantics without changing ordinary synthetic expression policy.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, binding: ProducerBinding) -> LoweredValue {
    lower_with_destination(ctx, expr, binding, None)
}

/// Retains destination-aware literal storage while selecting the declaration producer's policy.
pub(super) fn lower_for_destination(ctx: &mut LoweringContext<'_, '_>, expr: &Expr,
    binding: ProducerBinding, destination: &PhpType) -> LoweredValue {
    lower_with_destination(ctx, expr, binding, Some(destination))
}

/// Shares producer scoping between ordinary expression and destination-specific materialization.
fn lower_with_destination(ctx: &mut LoweringContext<'_, '_>, expr: &Expr,
    binding: ProducerBinding, destination: Option<&PhpType>) -> LoweredValue {
    let previous = ctx.source_mode;
    ctx.source_mode = expr.source_profile.mode;
    let result = crate::optimize::with_expression_semantics(ctx.semantic_profile, expr.source_profile, || {
        let mut original = expr.clone();
        original.producer_binding = None;
        let value = if let Some(destination) = destination {
            lower_expr_for_destination(ctx, &original, destination)
        } else { super::lower_expr_inner(ctx, &original) };
        if binding == ProducerBinding::IntegerToFloat { widen_integer(ctx, value, expr) }
        else { value }
    });
    ctx.source_mode = previous;
    result
}

/// Widens an integer payload while retaining null, objects and every other union alternative.
fn widen_integer(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    if ty == PhpType::Int { return coerce_to_float_at_span(ctx, value, Some(expr.span)); }
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar) { return value; }
    let temp = ctx.declare_synthetic_php_local(PhpType::Mixed);
    let value = coerce_value_for_temp(ctx, value, &PhpType::Mixed, expr.span);
    ctx.store_local(&temp, value, PhpType::Mixed, Some(expr.span));
    let variable = Expr::new(ExprKind::Variable(temp.clone()), expr.span);
    let condition = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("is_int"), args: vec![variable.clone()],
    }, expr.span);
    let converted = Expr::new(ExprKind::Cast { target: CastType::Float, expr: Box::new(variable.clone()) }, expr.span);
    let selection = Expr::new(ExprKind::Ternary {
        condition: Box::new(condition), then_expr: Box::new(converted), else_expr: Box::new(variable),
    }, expr.span);
    let result = lower_expr(ctx, &selection);
    let null = Expr::new(ExprKind::Null, expr.span);
    let null = lower_expr(ctx, &null);
    ctx.unset_local(&temp, null, Some(expr.span));
    result
}
