//! Purpose:
//! Closure, invokable-object, and dynamic-method expression calls.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a closure variable call.
pub(super) fn lower_closure_call(ctx: &mut LoweringContext<'_, '_>, var: &str, args: &[Expr], expr: &Expr) -> LoweredValue {
    if let Some(value) = lower_invokable_object_variable_call(ctx, var, args, expr) {
        return value;
    }
    let mut result_type = None;
    let mut instance_signature = None;
    if let Some(target) = ctx.static_callable_local(var) {
        result_type = Some(static_callable_return_type(ctx, &target));
        instance_signature = instance_callable_signature(&target).cloned();
        if let Some(value) = lower_static_callable_call(ctx, target, args, expr) {
            return value;
        }
    }
    let callable = ctx.load_local(var, Some(expr.span));
    let result_type = result_type.unwrap_or_else(|| dynamic_callable_result_type(ctx, callable.value, expr));
    if instance_signature.is_none() {
        let callable = root_descriptor_callback(ctx, callable, result_type, expr.span);
        let arg_container =
            lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span);
        return emit_callable_descriptor_invoke(ctx, callable, arg_container, expr.span);
    }
    let mut operands = vec![callable.value];
    operands.extend(lower_args_with_signature(ctx, instance_signature.as_ref(), args));
    ctx.emit_value(
        Op::ClosureCall,
        operands,
        callable_profile_immediate(),
        result_type,
        Op::ClosureCall.default_effects(),
        Some(expr.span),
    )
}

/// Lowers `$object(...)` when the local object has an `__invoke` method.
pub(super) fn lower_invokable_object_variable_call(
    ctx: &mut LoweringContext<'_, '_>,
    var: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object = Expr::new(ExprKind::Variable(var.to_string()), expr.span);
    lower_invokable_object_expr_call(ctx, &object, args, expr)
}

/// Lowers invokable object calls through the normal method-call path.
pub(super) fn lower_invokable_object_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !is_invokable_object_expr(ctx, callee) {
        return None;
    }
    Some(lower_method_call(ctx, callee, "__invoke", args, Op::MethodCall, expr))
}

/// Returns true when an expression is known to evaluate to an object with `__invoke`.
pub(super) fn is_invokable_object_expr(
    ctx: &LoweringContext<'_, '_>,
    callee: &Expr,
) -> bool {
    instance_callable_object_class(ctx, callee)
        .and_then(|class_name| class_method_signature(ctx, &class_name, "__invoke"))
        .is_some()
}

/// Lowers an expression call.
pub(super) fn lower_expr_call(ctx: &mut LoweringContext<'_, '_>, callee: &Expr, args: &[Expr], expr: &Expr) -> LoweredValue {
    if let Some(value) = lower_invokable_object_expr_call(ctx, callee, args, expr) {
        return value;
    }
    if let Some(value) = lower_first_class_callable_expr_call(ctx, callee, args, expr) {
        return value;
    }
    if let Some(value) = lower_literal_callable_array_expr_call(ctx, callee, args, expr) {
        return value;
    }
    if let Some(callback) = static_call_user_func_callback(ctx, callee) {
        if let Some(value) = lower_static_callable_call(ctx, callback, args, expr) {
            return value;
        }
    }
    if let Some(callback) = static_assignment_callable_target(ctx, callee) {
        lower_expr(ctx, callee);
        if let Some(value) = lower_static_callable_call(ctx, callback, args, expr) {
            return value;
        }
    }
    // `Closure::bind(fn &() => $this->prop, $obj, $obj)()` invokes the bound closure. Lower it
    // as a direct call to the closure with `$obj` boxed as its `$this` capture, so a
    // by-reference return passes the property's ref-cell pointer through (the generic runtime
    // descriptor invoker boxes results and cannot).
    if let Some(value) = lower_bound_closure_immediate_call(ctx, callee, args, expr) {
        return value;
    }
    let lowered_callee = lower_expr(ctx, callee);
    // An immediately-invoked closure literal (`(fn &() => …)()`) registers its static
    // callable binding while lowering. Call it directly through the static-callable path
    // (as `$f()` does) so the closure body's signature — including a by-reference return —
    // drives the call instead of the generic descriptor-invoke path, which cannot return
    // every result type.
    //
    // The direct call passes the closure's captured values, not its descriptor, so that
    // descriptor is unused by the invocation, but it is what keeps a captured environment
    // alive, and it must not be leaked either. Publish it for the duration of the call and
    // retire it afterwards, which also covers a throw from an argument or from the callee.
    // The binding is checked for direct callability first, so no instruction is emitted for
    // a call that then has to fall back to the descriptor path below.
    if let Some(target) = ctx.take_pending_static_callable_result() {
        if static_callable_call_lowers_directly(ctx, &target) {
            // Retiring the descriptor destroys the captured environment, which runs PHP
            // destructors that can throw into a catch in this same frame. The call's own owned
            // result is staged in a record published OUTSIDE the descriptor record, so it is
            // retired exactly once by that unwind instead of being stranded.
            let result_staging =
                prepublish_static_callable_call_result(ctx, &target, expr.span);
            let (_, descriptor_owner) = root_owned_call_operand(ctx, lowered_callee, expr.span);
            let value = lower_static_callable_call(ctx, target, args, expr)
                .expect("a directly callable static binding lowers its call");
            stage_call_result(ctx, result_staging.as_ref(), value, expr.span);
            if let Some(slot) = descriptor_owner {
                retire_owned_call_operand(ctx, slot, expr.span);
            }
            return take_prepublished_call_result(ctx, result_staging, value, expr.span);
        }
    }
    let result_type = dynamic_callable_result_type(ctx, lowered_callee.value, expr);
    let lowered_callee = root_descriptor_callback(ctx, lowered_callee, result_type, expr.span);
    let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span);
    emit_callable_descriptor_invoke(ctx, lowered_callee, arg_container, expr.span)
}

/// Recognizes the parser's internal `call_user_func([$object, $method], ...)`
/// desugaring for ordinary dynamic method syntax without changing explicit calls.
pub(super) fn lower_desugared_dynamic_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func" {
        return None;
    }
    let callback = args.first()?;
    if callback.span != expr.span {
        return None;
    }
    let ExprKind::ArrayLiteral(items) = &callback.kind else {
        return None;
    };
    let [object, method] = items.as_slice() else {
        return None;
    };
    Some(lower_dynamic_method_expr_call(
        ctx,
        object,
        method,
        &args[1..],
        expr,
    ))
}

/// Lowers `$object->{$method}(...)` as a dynamic method call, preserving PHP's
/// receiver/name evaluation before the null check and lazy argument evaluation.
pub(super) fn lower_dynamic_method_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    let method = lower_expr(ctx, method);
    let method_type = ctx.builder.value_php_type(method.value);
    let method_name = ctx.declare_hidden_temp(method_type.clone());
    store_value_into_temp(ctx, &method_name, method_type, method, expr.span);
    let method_expr = Expr::new(ExprKind::Variable(method_name), expr.span);
    let object_type = ctx.builder.value_php_type(object.value).codegen_repr();
    if !matches!(object_type, PhpType::Object(_))
        && !value_is_nullable(ctx, object.value)
        && !value_may_carry_container_miss(ctx, object.value)
    {
        return lower_dynamic_method_call_with_receiver(ctx, object, &method_expr, args, expr);
    }
    lower_nullable_dynamic_method_expr_call(ctx, object, &method_expr, args, expr)
}

/// Splits a dynamic method call so a null receiver throws before lowering any
/// call argument, while the already evaluated runtime method name is preserved.
pub(super) fn lower_nullable_dynamic_method_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let fatal_block = ctx
        .builder
        .create_named_block("dynamic_method.null.fatal", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("dynamic_method.non_null.call", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: fatal_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal_block);
    terminate_dynamic_method_call_on_null(ctx, method, expr);

    ctx.builder.position_at_end(call_block);
    lower_dynamic_method_call_with_receiver(ctx, object, method, args, expr)
}

/// Throws a catchable PHP `Error` with the runtime dynamic method name.
pub(super) fn terminate_dynamic_method_call_on_null(
    ctx: &mut LoweringContext<'_, '_>,
    method: &Expr,
    expr: &Expr,
) {
    let prefix = Expr::new(
        ExprKind::StringLiteral("Call to a member function ".to_string()),
        expr.span,
    );
    let prefix_and_method = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(prefix),
            op: BinOp::Concat,
            right: Box::new(method.clone()),
        },
        expr.span,
    );
    let suffix = Expr::new(ExprKind::StringLiteral("() on null".to_string()), expr.span);
    let message = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(prefix_and_method),
            op: BinOp::Concat,
            right: Box::new(suffix),
        },
        expr.span,
    );
    let message = lower_expr(ctx, &message);
    let message = ctx.emit_value(
        Op::StrPersist,
        vec![message.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(expr.span),
    );
    ctx.emit_void(
        Op::ThrowErrorValue,
        vec![message.value],
        None,
        Op::ThrowErrorValue.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::Unreachable);
}
