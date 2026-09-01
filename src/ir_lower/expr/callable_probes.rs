//! Purpose:
//! Static callable probes and eval-backed call_user_func fallbacks.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers direct function/static-method first-class callable probes for `is_callable()`.
pub(super) fn lower_static_is_callable(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "is_callable" || args.len() != 1 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    // Eval can declare callable targets after static metadata has been built.
    if ctx.has_eval_barrier() {
        return None;
    }
    match &args[0].kind {
        ExprKind::FirstClassCallable(
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. },
        ) => Some(emit_bool_literal(ctx, true, Some(expr.span))),
        ExprKind::ArrayLiteral(items) => {
            let is_callable = static_array_callable_is_callable(ctx, items)?;
            Some(emit_bool_literal(ctx, is_callable, Some(expr.span)))
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name).map(|target| {
            emit_bool_literal(
                ctx,
                static_callable_binding_is_callable(ctx, &target),
                Some(expr.span),
            )
        }),
        _ => None,
    }
}

/// Returns whether straight-line callable-local metadata represents a public callable.
pub(super) fn static_callable_binding_is_callable(
    ctx: &LoweringContext<'_, '_>,
    target: &StaticCallableBinding,
) -> bool {
    match target {
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_receiver_class_name(ctx, receiver)
                .is_some_and(|class_name| static_method_callback_is_callable(ctx, &class_name, method))
        }
        StaticCallableBinding::UserFunction(_)
        | StaticCallableBinding::ExternFunction(_)
        | StaticCallableBinding::Builtin(_)
        | StaticCallableBinding::Closure { .. }
        | StaticCallableBinding::InstanceMethod { .. } => true,
    }
}

/// Lowers static-string `call_user_func*` forms to direct call opcodes when possible.
pub(super) fn lower_static_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "call_user_func" => {
            let callback_expr = args.first()?;
            let callback_args = &args[1..];
            let signature = callable_descriptor_signature_for_expr(ctx, callback_expr);
            if call_user_func_should_use_descriptor(ctx, callback_expr, callback_args, signature.as_ref()) {
                return lower_call_user_func_descriptor_invoke(
                    ctx,
                    callback_expr,
                    callback_args,
                    signature.as_ref(),
                    expr,
                );
            }
            if let Some(callback) = instance_call_user_func_callback(ctx, callback_expr) {
                return lower_instance_callable_call_user_func(
                    ctx,
                    callback_expr,
                    callback,
                    callback_args,
                    expr,
                );
            }
            if let Some(callback) = static_call_user_func_callback(ctx, callback_expr) {
                return lower_static_callable_call(ctx, callback, callback_args, expr);
            }
            lower_eval_call_user_func_fallback(ctx, callback_expr, callback_args, expr)
        }
        "call_user_func_array" => {
            let [callback_arg, arg_array] = args else {
                return None;
            };
            if let ExprKind::StringLiteral(callback_name) = &callback_arg.kind {
                let callback_key = php_symbol_key(callback_name.trim_start_matches('\\'));
                if matches!(
                    callback_key.as_str(),
                    "mktime" | "gmmktime" | "__elephc_mktime_raw" | "__elephc_gmmktime_raw"
                ) {
                    let internal = if callback_key.contains("gmmktime") {
                        "__elephc_gmmktime_raw"
                    } else {
                        "__elephc_mktime_raw"
                    };
                    let direct = Expr::new(
                        ExprKind::FunctionCall {
                            name: Name::unqualified(internal),
                            args: (0..6)
                                .map(|index| {
                                    Expr::new(
                                        ExprKind::ArrayAccess {
                                            array: Box::new(arg_array.clone()),
                                            index: Box::new(Expr::new(
                                                ExprKind::IntLiteral(index),
                                                arg_array.span,
                                            )),
                                        },
                                        arg_array.span,
                                    )
                                })
                                .collect(),
                        },
                        expr.span,
                    );
                    return Some(lower_expr(ctx, &direct));
                }
            }
            if matches!(arg_array.kind, ExprKind::ArrayLiteralAssoc(_))
                && static_callable_binding_for_expr(ctx, callback_arg)
                    .is_some_and(|target| matches!(target, StaticCallableBinding::InstanceMethod { .. }))
            {
                return None;
            }
            if let Some(callback_args) = static_call_user_func_array_args(arg_array) {
                if let Some(callback) = instance_call_user_func_callback(ctx, callback_arg) {
                    return lower_instance_callable_call_user_func(
                        ctx,
                        callback_arg,
                        callback,
                        &callback_args,
                        expr,
                    );
                }
                if let Some(callback) = static_call_user_func_callback(ctx, callback_arg) {
                    return lower_static_callable_call(ctx, callback, &callback_args, expr);
                }
            }
            lower_eval_call_user_func_array_fallback(ctx, callback_arg, arg_array, expr)
        }
        _ => None,
    }
}

/// Lowers unresolved string callbacks after an eval barrier through the eval function table.
pub(super) fn lower_eval_call_user_func_fallback(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.has_eval_barrier() || !plain_positional_call_args(callback_args) {
        return None;
    }
    let ExprKind::StringLiteral(callback_name) = &callback_expr.kind else {
        return None;
    };
    if callback_name.contains("::")
        || resolve_static_string_callable(ctx, callback_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(callback_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    let operands = lower_args(ctx, callback_args);
    Some(ctx.emit_value(
        Op::EvalFunctionCall,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::EvalFunctionCall.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers unresolved `call_user_func_array()` string callbacks through the eval table.
pub(super) fn lower_eval_call_user_func_array_fallback(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    arg_array: &Expr,
    expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.has_eval_barrier() {
        return None;
    }
    let ExprKind::StringLiteral(callback_name) = &callback_expr.kind else {
        return None;
    };
    if callback_name.contains("::")
        || resolve_static_string_callable(ctx, callback_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(callback_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    let arg_array = lower_expr(ctx, arg_array);
    let arg_array = coerce_eval_function_arg_array(ctx, arg_array, expr.span);
    Some(ctx.emit_value(
        Op::EvalFunctionCallArray,
        vec![arg_array.value],
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::EvalFunctionCallArray.default_effects(),
        Some(expr.span),
    ))
}

/// Boxes a post-barrier dynamic-call argument container for the eval bridge ABI.
pub(super) fn coerce_eval_function_arg_array(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return value;
    }
    ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}
