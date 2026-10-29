//! Purpose:
//! ReflectionMethod invocation dispatch and argument normalization.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers reflected method invocation for statically-known `ReflectionMethod` objects.
pub(super) fn lower_reflection_method_invoke_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let method_key = php_symbol_key(method);
    let object_expr = object_expr?;
    let (class_name, reflected_method) = reflection_method_reflected_target(ctx, object_expr)?;
    let Some((object_arg, forwarded_args)) = (match method_key.as_str() {
        "invoke" => reflection_method_invoke_args(args),
        "invokeargs" => reflection_method_invoke_args_array(ctx, args),
        _ => return None,
    }) else {
        return Some(lower_reflection_method_invoke_unsupported(
            ctx,
            &method_key,
            expr,
        ));
    };
    let Some(target_kind) = reflection_method_target_kind(ctx, &class_name, &reflected_method)
    else {
        return Some(lower_reflection_method_invoke_unsupported(
            ctx,
            &method_key,
            expr,
        ));
    };
    match target_kind {
        ReflectionMethodTargetKind::Static => Some(lower_reflection_static_method_invoke(
            ctx,
            &class_name,
            &reflected_method,
            &object_arg,
            &forwarded_args,
            expr,
        )),
        ReflectionMethodTargetKind::Instance => Some(lower_reflection_instance_method_invoke(
            ctx,
            &reflected_method,
            &object_arg,
            &forwarded_args,
            expr,
        )),
    }
}

/// Lowers a static reflected-method invocation after evaluating the ignored object slot.
pub(super) fn lower_reflection_static_method_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    reflected_method: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let ignored_object = lower_expr(ctx, object_arg);
    if ctx.value_is_owning_temporary(ignored_object) {
        crate::ir_lower::ownership::release_if_owned(ctx, ignored_object, Some(object_arg.span));
    }
    let receiver = StaticReceiver::Named(Name::from(class_name.to_string()));
    lower_static_method_call(ctx, &receiver, reflected_method, forwarded_args, expr)
}

/// Lowers an instance reflected-method invocation using the first invoke argument as receiver.
pub(super) fn lower_reflection_instance_method_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    reflected_method: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object_arg);
    if value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        terminate_method_call_on_null(ctx, reflected_method);
        return null_value;
    }
    if value_is_nullable(ctx, object.value) {
        return lower_nullable_regular_method_call(
            ctx,
            object,
            reflected_method,
            forwarded_args,
            expr,
        );
    }
    // The source receiver is the ReflectionMethod object; this is its selected target.
    lower_method_call_with_receiver_direct(
        ctx,
        object,
        reflected_method,
        forwarded_args,
        Op::MethodCall,
        expr,
    )
}

/// Splits `ReflectionMethod::invoke($object, ...$args)` into receiver and method args.
pub(super) fn reflection_method_invoke_args(args: &[Expr]) -> Option<(Expr, Vec<Expr>)> {
    let args = reflection_class_new_instance_args(args);
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [object, forwarded @ ..] => Some((object.clone(), forwarded.to_vec())),
            _ => None,
        };
    }
    let mut object = None;
    let mut forwarded = Vec::new();
    let mut args = args.into_iter();
    if let Some(first) = args.next() {
        match first.kind {
            ExprKind::NamedArg {
                ref name,
                ref value,
            } if php_symbol_key(name) == "object" => {
                object = Some((**value).clone());
            }
            ExprKind::NamedArg { .. } => forwarded.push(first),
            _ => object = Some(first),
        }
    }
    for arg in args {
        match arg.kind {
            ExprKind::NamedArg {
                ref name,
                ref value,
            } if php_symbol_key(name) == "object" => {
                if object.replace((**value).clone()).is_some() {
                    return None;
                }
            }
            _ => forwarded.push(arg),
        }
    }
    object.map(|object| (object, forwarded))
}

/// Splits `ReflectionMethod::invokeArgs($object, $args)` into receiver and method args.
pub(super) fn reflection_method_invoke_args_array(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<(Expr, Vec<Expr>)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [object, forwarded] => {
                let forwarded = reflection_class_new_instance_args_value(ctx, forwarded)?;
                Some((object.clone(), forwarded))
            }
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionMethod")
        .and_then(|class_info| class_info.methods.get(&php_symbol_key("invokeArgs")))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    let object = planned_regular_arg_expr(plan.regular_args.first()?)?.clone();
    let forwarded_arg = planned_regular_arg_expr(plan.regular_args.get(1)?)?;
    let forwarded = reflection_class_new_instance_args_value(ctx, forwarded_arg)?;
    Some((object, forwarded))
}

/// Classifies whether a known reflected method is static or instance-dispatched.
pub(super) fn reflection_method_target_kind(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> Option<ReflectionMethodTargetKind> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let method_key = php_symbol_key(method);
    if class_info.static_methods.contains_key(&method_key) {
        return Some(ReflectionMethodTargetKind::Static);
    }
    if class_info.methods.contains_key(&method_key) {
        return Some(ReflectionMethodTargetKind::Instance);
    }
    None
}

/// Dispatch kind for a statically-known reflected method.
#[derive(Clone, Copy)]
pub(super) enum ReflectionMethodTargetKind {
    Instance,
    Static,
}

/// Emits a runtime fatal for ReflectionMethod invocation forms not yet lowered.
pub(super) fn lower_reflection_method_invoke_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    method_key: &str,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let method_name = if method_key == "invokeargs" {
        "invokeArgs"
    } else {
        "invoke"
    };
    let message = ctx.intern_string(&format!(
        "Fatal error: unsupported ReflectionMethod::{}() target or argument forwarding\n",
        method_name
    ));
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

