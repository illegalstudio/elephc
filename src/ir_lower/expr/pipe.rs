//! Purpose:
//! Pipe expression lowering and callable dispatch preparation.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a pipe operation.
pub(super) fn lower_pipe(ctx: &mut LoweringContext<'_, '_>, value: &Expr, callable: &Expr, expr: &Expr) -> LoweredValue {
    match &callable.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            let arg = lower_pipe_value_temp(ctx, value, expr);
            let synthetic = Expr::new(
                ExprKind::FunctionCall {
                    name: name.clone(),
                    args: vec![arg],
                },
                expr.span,
            );
            lower_expr(ctx, &synthetic)
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            let arg = lower_pipe_value_temp(ctx, value, expr);
            let synthetic = Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: receiver.clone(),
                    method: method.clone(),
                    args: vec![arg],
                },
                expr.span,
            );
            lower_expr(ctx, &synthetic)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            let arg = lower_pipe_value_temp(ctx, value, expr);
            let synthetic = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: method.clone(),
                    args: vec![arg],
                },
                expr.span,
            );
            lower_expr(ctx, &synthetic)
        }
        ExprKind::Variable(name) => lower_pipe_callable_variable(ctx, value, name, expr),
        _ => lower_pipe_runtime_call(ctx, value, callable, expr),
    }
}

/// Lowers `value |> $callable` when the local still has straight-line callable metadata.
pub(super) fn lower_pipe_callable_variable(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    name: &str,
    expr: &Expr,
) -> LoweredValue {
    let arg = lower_pipe_value_temp(ctx, value, expr);
    let callable = Expr::new(ExprKind::Variable(name.to_string()), expr.span);
    let Some(target) = ctx.static_callable_local(name) else {
        return lower_pipe_runtime_call(ctx, &arg, &callable, expr);
    };
    if matches!(target, StaticCallableBinding::InstanceMethod { .. }) {
        emit_backend_comment_marker(ctx, &format!("call descriptor variable ${}()", name), expr.span);
        return lower_pipe_runtime_call(ctx, &arg, &callable, expr);
    }
    emit_backend_comment_marker(
        ctx,
        &format!("uninvoked FCC wrapper ${} (stubbed by EIR direct pipe call)", name),
        expr.span,
    );
    let fallback_arg = arg.clone();
    lower_static_callable_call(ctx, target, &[arg], expr).unwrap_or_else(|| {
        lower_pipe_runtime_call(ctx, &fallback_arg, &callable, expr)
    })
}

/// Emits a backend-only comment marker using a void EIR NOP instruction.
pub(super) fn emit_backend_comment_marker(ctx: &mut LoweringContext<'_, '_>, message: &str, span: Span) {
    let data = ctx.intern_string(message);
    ctx.emit_void(
        Op::Nop,
        Vec::new(),
        Some(Immediate::Data(data)),
        Op::Nop.default_effects(),
        Some(span),
    );
}

/// Lowers the pipe input once, stores it in a hidden local, and returns a temp argument expression.
pub(super) fn lower_pipe_value_temp(ctx: &mut LoweringContext<'_, '_>, value: &Expr, expr: &Expr) -> Expr {
    let value = lower_expr(ctx, value);
    let temp_type = ctx.builder.value_php_type(value.value);
    let temp_name = ctx.declare_hidden_temp(temp_type.clone());
    store_value_into_temp(ctx, &temp_name, temp_type, value, expr.span);
    Expr::new(ExprKind::Variable(temp_name), expr.span)
}

/// Lowers pipe shapes that still need a dynamic callable invocation backend path.
pub(super) fn lower_pipe_runtime_call(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    callable: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let result_type = pipe_runtime_result_type(ctx, callable, expr);
    let value = lower_expr(ctx, value);
    let callable = lower_expr(ctx, callable);
    emit_descriptor_invoker_value(
        ctx,
        Op::PipeCall,
        vec![value.value, callable.value],
        None,
        result_type,
        expr.span,
    )
}

/// Returns the best known result type for a runtime-lowered pipe call.
pub(super) fn pipe_runtime_result_type(
    ctx: &LoweringContext<'_, '_>,
    callable: &Expr,
    expr: &Expr,
) -> PhpType {
    match &callable.kind {
        ExprKind::Variable(name) => ctx
            .static_callable_local(name)
            .map(|target| static_callable_return_type(ctx, &target))
            .unwrap_or_else(|| fallback_expr_type(expr)),
        _ => fallback_expr_type(expr),
    }
}
