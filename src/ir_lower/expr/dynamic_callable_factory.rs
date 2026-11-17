//! Purpose:
//! Lowers dynamic callable creation with catchable PHP errors and owned descriptors.
//!
//! Called from:
//! - Static method lowering for the parser marker and Closure::fromCallable.
//!
//! Key details:
//! - The source is evaluated and rooted once before validation and normalization.

use super::*;

/// Validates the selected callable before acquiring its returned descriptor owner.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, source: &Expr, method: &str, expr: &Expr,
) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Callable, expr.span);
    let value = lower_expr(ctx, source);
    let (value, owner) = root_closure_bind_operand(ctx, value, expr.span);
    let dynamic = method == crate::parser::ast::DYNAMIC_CALLABLE_FACTORY;
    let exception = if dynamic { "Error" } else { "TypeError" };
    let message = if dynamic { "Value is not callable" } else { "Failed to create closure from callable" };
    let ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let target = crate::ir::RuntimeFnId::IsCallable;
    let valid = ctx.emit_value(Op::RuntimeCall, vec![value.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
        PhpType::Bool, target.effects(), Some(expr.span));
    let accepted = ctx.builder.create_named_block("callable.accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("callable.rejected", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: valid.value,
        then_target: accepted, then_args: Vec::new(), else_target: rejected, else_args: Vec::new() });
    ctx.builder.position_at_end(rejected);
    crate::ir_lower::stmt::lower_throw_access_exception(ctx, exception, message, expr.span);
    ctx.builder.position_at_end(accepted);
    let descriptor = if matches!(ty, PhpType::Callable | PhpType::Str | PhpType::Mixed | PhpType::Union(_)
        | PhpType::Object(_)) || matches!(&ty, PhpType::Array(element)
            if matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Str))
    {
        ctx.emit_owned_value(Op::NormalizeCallable, vec![value.value], None,
            PhpType::Callable, Op::NormalizeCallable.default_effects(), Some(expr.span))
    } else {
        // Statically invalid storage cannot reach this edge after is_callable rejected it.
        ctx.emit_value(Op::ConstNull, Vec::new(), None, PhpType::Callable,
            Op::ConstNull.default_effects(), Some(expr.span))
    };
    stage_call_result(ctx, staging.as_ref(), descriptor, expr.span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, expr.span); }
    take_prepublished_call_result(ctx, staging, descriptor, expr.span)
}
