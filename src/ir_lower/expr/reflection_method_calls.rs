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
    if method_key == "getclosure" {
        let object_arg = reflection_method_get_closure_object(ctx, args)?;
        let target_kind = reflection_method_target_kind(ctx, &class_name, &reflected_method)?;
        let target = match target_kind {
            ReflectionMethodTargetKind::Static => {
                let ignored_object = lower_expr(ctx, &object_arg);
                if ctx.value_is_owning_temporary(ignored_object) {
                    crate::ir_lower::ownership::release_if_owned(
                        ctx,
                        ignored_object,
                        Some(object_arg.span),
                    );
                }
                CallableTarget::StaticMethod {
                    receiver: StaticReceiver::Named(Name::from(class_name)),
                    method: reflected_method,
                }
            }
            ReflectionMethodTargetKind::Instance => CallableTarget::Method {
                object: Box::new(object_arg),
                method: reflected_method,
            },
        };
        return Some(lower_first_class_callable(ctx, &target, expr));
    }
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
    // php binds a reflected method by its DECLARING class, so a private method of an ancestor
    // runs on a descendant receiver. A direct `MethodCall` resolves the name on the receiver's
    // own class and found nothing ("method call to unknown method AppK::secret"); the shell
    // body's `call_user_func_array` resolves it by the reflected class at run time instead.
    if matches!(target_kind, ReflectionMethodTargetKind::Instance)
        && reflected_private_method_escapes_receiver(ctx, &class_name, &reflected_method, &object_arg)
    {
        return None;
    }
    match target_kind {
        ReflectionMethodTargetKind::Static => Some(lower_reflection_static_method_invoke(
            ctx,
            &class_name,
            &reflected_method,
            &object_arg,
            &forwarded_args,
            expr,
        )),
        ReflectionMethodTargetKind::Instance => {
            // php runs the REFLECTED class's method on the receiver, even when the receiver's
            // class overrides it: `(new ReflectionMethod(P::class, 'get'))->invoke(new C())` is
            // `P::get`, not `C::get`. A plain `MethodCall` would dispatch virtually.
            let direct = reflected_method_has_descendant_override(ctx, &class_name, &reflected_method)
                .then(|| class_name.trim_start_matches('\\').to_string());
            Some(lower_reflection_instance_method_invoke_scoped(
                ctx,
                &reflected_method,
                &object_arg,
                &forwarded_args,
                expr,
                direct,
            ))
        }
    }
}

/// Returns whether a descendant of `class_name` redeclares the instance method `method`, so a
/// virtual call on a descendant receiver would not reach the reflected implementation.
fn reflected_method_has_descendant_override(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> bool {
    let declaring = class_name.trim_start_matches('\\');
    let key = php_symbol_key(method);
    let Some(info) = ctx.classes.get(declaring) else {
        return false;
    };
    if info.abstract_methods.contains(&key) || !info.methods.contains_key(&key) {
        return false;
    }
    let reflected_owner = info.method_declaring_classes.get(&key);
    ctx.classes.iter().any(|(name, candidate)| {
        name != declaring
            && candidate.methods.contains_key(&key)
            && candidate.method_declaring_classes.get(&key) != reflected_owner
            && {
                let mut ancestor = candidate.parent.as_deref();
                let mut found = false;
                while let Some(parent) = ancestor {
                    if parent == declaring {
                        found = true;
                        break;
                    }
                    ancestor = ctx.classes.get(parent).and_then(|info| info.parent.as_deref());
                }
                found
            }
    })
}

/// Returns whether a reflected PRIVATE instance method cannot be dispatched statically on the
/// receiver: private methods are not inherited into a subclass's method table, so any receiver
/// whose class is not the declaring class (or is not known at all) must go through run-time
/// resolution by the reflected class.
fn reflected_private_method_escapes_receiver(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
    receiver: &Expr,
) -> bool {
    let declaring = class_name.trim_start_matches('\\');
    let is_private = ctx
        .classes
        .get(declaring)
        .and_then(|info| info.method_visibilities.get(&php_symbol_key(method)))
        == Some(&Visibility::Private);
    if !is_private {
        return false;
    }
    match instance_callable_object_class(ctx, receiver) {
        Some(receiver_class) => receiver_class.trim_start_matches('\\') != declaring,
        None => true,
    }
}

/// Normalizes the optional receiver accepted by `ReflectionMethod::getClosure()`.
fn reflection_method_get_closure_object(
    _ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    match args.as_slice() {
        [] => Some(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
        [arg] => match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "object" => {
                Some((**value).clone())
            }
            ExprKind::NamedArg { .. } => None,
            _ => Some(arg.clone()),
        },
        _ => None,
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

/// Lowers an instance reflected-method invocation using the first invoke argument as receiver,
/// optionally pinning the call to the implementation `direct_class` names instead of dispatching
/// on the receiver's own class.
fn lower_reflection_instance_method_invoke_scoped(
    ctx: &mut LoweringContext<'_, '_>,
    reflected_method: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
    direct_class: Option<String>,
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
    // Only a receiver the backend can type as an object takes the pinned call.
    if matches!(ctx.builder.value_php_type(object.value), PhpType::Object(_)) {
        ctx.direct_method_call_class = direct_class;
    }
    let call = lower_method_call_with_receiver(
        ctx,
        object,
        reflected_method,
        forwarded_args,
        Op::MethodCall,
        expr,
    );
    ctx.direct_method_call_class = None;
    call
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
