//! Purpose:
//! Normalizes weak Stringable objects at registry-owned string-union argument boundaries.
//!
//! Called from:
//! - Builtin argument preparation after shared named and spread planning.
//!
//! Key details:
//! - The source argument is evaluated once, copied into a scoped PHP local, and rooted.
//! - Ordinary EIR string casts own conversion results and preserve callback exceptions.
//! - Strict callers and non-Stringable alternatives retain their existing validation path.

use super::*;

/// Converts only weak Stringable objects while keeping other accepted union members unchanged.
pub(super) fn lower_weak_string_union_operand(
    ctx: &mut LoweringContext<'_, '_>, args: &[Expr], mut operands: Vec<ValueId>, arg: usize,
) -> Vec<ValueId> {
    if crate::source::current_strict_types() { return operands; }
    let Some(&operand) = operands.get(arg) else { return operands; };
    let ty = ctx.builder.value_php_type(operand);
    if !matches!(ty.codegen_repr(), PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)) {
        return operands;
    }
    let span = args.get(arg).or_else(|| args.first()).map(|arg| arg.span).unwrap_or_else(Span::dummy);
    let ir_type = ctx.builder.value_type(operand);
    let source = root_evaluated_call_argument(ctx,
        LoweredValue { value: operand, ir_type }, span);
    let borrowed = ctx.emit_value(Op::Borrow, vec![source.value], None, ty.clone(),
        Op::Borrow.default_effects(), Some(span));
    // Mixed string casts track the detached callback result as an owning temporary.
    let borrowed = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        borrowed
    } else {
        ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span))
    };
    let ty = PhpType::Mixed;
    let copied = crate::ir_lower::ownership::copy_assignment_value(ctx, borrowed, Some(span));
    let name = ctx.declare_synthetic_php_local(ty.clone());
    store_value_into_temp(ctx, &name, ty, copied, span);
    publish_scoped_value_local(ctx, &name, span);
    let variable = Expr::new(ExprKind::Variable(name), span);
    let condition = Expr::new(ExprKind::InstanceOf {
        value: Box::new(variable.clone()),
        target: InstanceOfTarget::Name(Name::unqualified("Stringable")),
    }, span);
    let converted = Expr::new(ExprKind::Cast {
        target: CastType::String, expr: Box::new(variable.clone()),
    }, span);
    let normalized = lower_expr(ctx, &Expr::new(ExprKind::Ternary {
        condition: Box::new(condition), then_expr: Box::new(converted), else_expr: Box::new(variable),
    }, span));
    operands[arg] = root_evaluated_call_argument(ctx, normalized, span).value;
    operands
}
