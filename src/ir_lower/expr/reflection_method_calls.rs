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
    let (requested_class, reflected_method) = reflection_method_reflected_target(ctx, object_expr)?;
    // A ReflectionMethod is bound to the effective implementation selected when it is
    // constructed. Calling through the requested descendant would re-enter virtual dispatch and
    // can therefore select a later override instead of the reflected method.
    let class_name = reflected_method_implementation_class(ctx, &requested_class, &reflected_method);
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
            &class_name,
            &reflected_method,
            &object_arg,
            &forwarded_args,
            expr,
        )),
    }
}

/// Resolves the implementation a statically-known ReflectionMethod must invoke exactly.
///
/// `method_impl_classes` records inherited and synthesized implementations separately from the
/// requested class. Retaining that owner mirrors php-src's stored `zend_function *` target while
/// still allowing the normal receiver compatibility guard to run at invocation time.
fn reflected_method_implementation_class(
    ctx: &LoweringContext<'_, '_>,
    requested_class: &str,
    reflected_method: &str,
) -> String {
    let method_key = php_symbol_key(reflected_method);
    ctx.classes
        .get(requested_class.trim_start_matches('\\'))
        .and_then(|info| info.method_impl_classes.get(&method_key))
        .cloned()
        .unwrap_or_else(|| requested_class.to_string())
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
    declaring_class: &str,
    reflected_method: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    if is_reflected_builtin_date_serialize(declaring_class, reflected_method) {
        return lower_reflection_builtin_date_serialize_invoke(
            ctx,
            declaring_class,
            object_arg,
            forwarded_args,
            expr,
        );
    }
    lower_reflection_exact_instance_method_invoke(
        ctx,
        declaring_class,
        reflected_method,
        object_arg,
        forwarded_args,
        expr,
    )
}

/// Lowers a statically-known ReflectionMethod target without virtual override dispatch.
fn lower_reflection_exact_instance_method_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    reflected_method: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object_arg);
    if value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        lower_reflection_exact_method_without_object(
            ctx,
            Some(object),
            declaring_class,
            reflected_method,
            expr,
        );
        return null_value;
    }
    lower_reflection_exact_instance_method_checked(
        ctx,
        declaring_class,
        reflected_method,
        object,
        forwarded_args,
        expr,
    )
}

/// Returns whether reflection must call one exact ext/date `__serialize()` implementation.
fn is_reflected_builtin_date_serialize(declaring_class: &str, method: &str) -> bool {
    php_symbol_key(method) == "__serialize"
        && matches!(
            declaring_class.trim_start_matches('\\'),
            "DateTime" | "DateTimeImmutable" | "DateTimeZone" | "DateInterval" | "DatePeriod"
        )
}

/// Validates a reflected receiver and emits the statically captured implementation exactly.
fn lower_reflection_exact_instance_method_checked(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    reflected_method: &str,
    object: LoweredValue,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let method_key = php_symbol_key(reflected_method);
    let raw_result_type = class_method_signature(ctx, declaring_class, &method_key)
        .map(|signature| normalize_value_php_type(signature.return_type.clone()))
        .unwrap_or_else(|| fallback_expr_type(expr));
    let finalize_date_serialize = reflection_date_serialize_requires_runtime_finalizer(
        ctx,
        declaring_class,
        &method_key,
    );
    let result_type = if finalize_date_serialize {
        PhpType::Mixed
    } else {
        raw_result_type.clone()
    };
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let null_block = ctx
        .builder
        .create_named_block("reflection.exact_method.null", Vec::new());
    let compatibility_block = ctx
        .builder
        .create_named_block("reflection.exact_method.compatibility", Vec::new());
    let incompatible_block = ctx
        .builder
        .create_named_block("reflection.exact_method.incompatible", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("reflection.exact_method.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("reflection.exact_method.merge", Vec::new());
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
        then_target: null_block,
        then_args: Vec::new(),
        else_target: compatibility_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    lower_reflection_exact_method_without_object(
        ctx,
        Some(object),
        declaring_class,
        reflected_method,
        expr,
    );

    ctx.builder.position_at_end(compatibility_block);
    let class_data = ctx.intern_class_name(declaring_class);
    let is_compatible = ctx.emit_value(
        Op::InstanceOf,
        vec![object.value],
        Some(Immediate::Data(class_data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_compatible.value,
        then_target: call_block,
        then_args: Vec::new(),
        else_target: incompatible_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(incompatible_block);
    lower_reflection_date_serialize_incompatible_receiver(ctx, object, expr);

    ctx.builder.position_at_end(call_block);
    let original_object = object;
    let needs_mixed_unbox = matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );
    let mixed_source_is_owned = needs_mixed_unbox && ctx.value_is_owning_temporary(object);
    let object = if needs_mixed_unbox {
        ctx.emit_value(
            Op::RuntimeCall,
            vec![object.value],
            None,
            PhpType::Object(declaring_class.to_string()),
            effects_lookup::runtime_effects(),
            Some(expr.span),
        )
    } else {
        object
    };
    if mixed_source_is_owned {
        crate::ir_lower::ownership::release_if_owned(ctx, original_object, Some(expr.span));
    }
    let call = lower_exact_reflection_instance_method(
        ctx,
        declaring_class,
        reflected_method,
        object,
        forwarded_args,
        raw_result_type,
        finalize_date_serialize,
        expr,
    );
    store_value_into_temp(ctx, &temp_name, result_type, call, expr.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Emits the exact method call and, for DateTime user overrides, runtime-kind finalization.
pub(super) fn lower_exact_reflection_instance_method(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    reflected_method: &str,
    object: LoweredValue,
    forwarded_args: &[Expr],
    raw_result_type: PhpType,
    finalize_date_serialize: bool,
    expr: &Expr,
) -> LoweredValue {
    let method_key = php_symbol_key(reflected_method);
    let sig = class_method_signature(ctx, declaring_class, &method_key).cloned();
    let arg_values = lower_args_with_signature(ctx, sig.as_ref(), forwarded_args);
    let mut operands = vec![object.value];
    operands.extend(arg_values.iter().copied());
    let target = ctx.intern_string(&format!("{}::{}", declaring_class, reflected_method));
    let raw_call = ctx.emit_value(
        Op::MethodCallExact,
        operands,
        Some(Immediate::Data(target)),
        raw_result_type,
        Op::MethodCallExact.default_effects(),
        Some(expr.span),
    );
    let call = if finalize_date_serialize {
        ctx.emit_value(
            Op::RuntimeCall,
            vec![raw_call.value, object.value],
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::DateSerializeFinalize,
            )),
            PhpType::Mixed,
            effects_lookup::runtime_effects(),
            Some(expr.span),
        )
    } else {
        raw_call
    };
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        &arg_values,
        Some(call.value),
        &ReturnArgAlias::None,
        sig.as_ref(),
        expr.span,
    );
    release_owning_receiver_temporary(ctx, object, expr.span);
    call
}

/// Returns whether a DateTime descendant's user serializer needs runtime array-kind boxing.
fn reflection_date_serialize_requires_runtime_finalizer(
    ctx: &LoweringContext<'_, '_>,
    declaring_class: &str,
    method_key: &str,
) -> bool {
    if method_key != "__serialize" || is_reflected_builtin_date_serialize(declaring_class, method_key)
    {
        return false;
    }
    let mut current = Some(declaring_class.trim_start_matches('\\'));
    while let Some(class_name) = current {
        if matches!(
            class_name,
            "DateTime" | "DateTimeImmutable" | "DateTimeZone" | "DateInterval" | "DatePeriod"
        ) {
            return true;
        }
        current = ctx
            .classes
            .get(class_name)
            .and_then(|info| info.parent.as_deref())
            .map(|parent| parent.trim_start_matches('\\'));
    }
    false
}

/// Raises ReflectionMethod's exception for an exact instance target without a receiver object.
fn lower_reflection_exact_method_without_object(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: Option<LoweredValue>,
    declaring_class: &str,
    reflected_method: &str,
    expr: &Expr,
) {
    if let Some(receiver) = receiver {
        if ctx.value_is_owning_temporary(receiver) {
            crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(expr.span));
        }
    }
    let message = format!(
        "Trying to invoke non static method {}::{}() without an object",
        declaring_class.trim_start_matches('\\'),
        reflected_method
    );
    let exception = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("ReflectionException"),
            args: vec![Expr::new(ExprKind::StringLiteral(message), expr.span)],
        },
        expr.span,
    );
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw {
        value: exception.value,
    });
}

/// Lowers ReflectionMethod invocation of a built-in DateTime serializer without virtual override dispatch.
fn lower_reflection_builtin_date_serialize_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object_arg);
    if value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        lower_reflection_date_serialize_without_object(ctx, Some(object), declaring_class, expr);
        return null_value;
    }
    lower_reflection_builtin_date_serialize_checked(
        ctx,
        declaring_class,
        object,
        forwarded_args,
        expr,
    )
}

/// Validates one reflected serializer receiver, then calls the declaring implementation exactly.
fn lower_reflection_builtin_date_serialize_checked(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    object: LoweredValue,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let result_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let null_block = ctx
        .builder
        .create_named_block("reflection.date_serialize.null", Vec::new());
    let compatibility_block = ctx
        .builder
        .create_named_block("reflection.date_serialize.compatibility", Vec::new());
    let incompatible_block = ctx
        .builder
        .create_named_block("reflection.date_serialize.incompatible", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("reflection.date_serialize.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("reflection.date_serialize.merge", Vec::new());
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
        then_target: null_block,
        then_args: Vec::new(),
        else_target: compatibility_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(null_block);
    lower_reflection_date_serialize_without_object(ctx, Some(object), declaring_class, expr);
    ctx.builder.position_at_end(compatibility_block);
    let class_data = ctx.intern_class_name(declaring_class);
    let is_compatible = ctx.emit_value(
        Op::InstanceOf,
        vec![object.value],
        Some(Immediate::Data(class_data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_compatible.value,
        then_target: call_block,
        then_args: Vec::new(),
        else_target: incompatible_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(incompatible_block);
    lower_reflection_date_serialize_incompatible_receiver(ctx, object, expr);
    ctx.builder.position_at_end(call_block);
    let original_object = object;
    let needs_mixed_unbox = matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );
    let mixed_source_is_owned = needs_mixed_unbox && ctx.value_is_owning_temporary(object);
    let object = if needs_mixed_unbox {
        ctx.emit_value(
            Op::RuntimeCall,
            vec![object.value],
            None,
            PhpType::Object(declaring_class.to_string()),
            effects_lookup::runtime_effects(),
            Some(expr.span),
        )
    } else {
        object
    };
    if mixed_source_is_owned {
        crate::ir_lower::ownership::release_if_owned(ctx, original_object, Some(expr.span));
    }
    let call = lower_exact_reflection_builtin_date_serialize(
        ctx,
        declaring_class,
        object,
        forwarded_args,
        expr,
    );
    store_value_into_temp(ctx, &temp_name, result_type, call, expr.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Throws the PHP ReflectionException used when invoke receives an incompatible object.
fn lower_reflection_date_serialize_incompatible_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    expr: &Expr,
) {
    if ctx.value_is_owning_temporary(receiver) {
        crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(expr.span));
    }
    let exception = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("ReflectionException"),
            args: vec![Expr::new(
                ExprKind::StringLiteral(
                    "Given object is not an instance of the class this method was declared in"
                        .to_string(),
                ),
                expr.span,
            )],
        },
        expr.span,
    );
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw {
        value: exception.value,
    });
}

/// Throws ReflectionMethod's PHP exception for invoking an instance method without an object.
fn lower_reflection_date_serialize_without_object(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: Option<LoweredValue>,
    declaring_class: &str,
    expr: &Expr,
) {
    if let Some(receiver) = receiver {
        if ctx.value_is_owning_temporary(receiver) {
            crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(expr.span));
        }
    }
    let message = format!(
        "Trying to invoke non static method {}::__serialize() without an object",
        declaring_class.trim_start_matches('\\')
    );
    let exception = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("ReflectionException"),
            args: vec![Expr::new(ExprKind::StringLiteral(message), expr.span)],
        },
        expr.span,
    );
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw {
        value: exception.value,
    });
}

/// Emits an exact reflected DateTime serializer call and forcibly merges the concrete object's props.
fn lower_exact_reflection_builtin_date_serialize(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    object: LoweredValue,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let method_key = php_symbol_key("__serialize");
    let sig = class_method_signature(ctx, declaring_class, &method_key).cloned();
    let arg_values = lower_args_with_signature(ctx, sig.as_ref(), forwarded_args);
    let mut operands = vec![object.value];
    operands.extend(arg_values.iter().copied());
    let result_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let target = ctx.intern_string(&format!("{}::__serialize", declaring_class));
    let raw_call = ctx.emit_value(
        Op::MethodCallExact,
        operands,
        Some(Immediate::Data(target)),
        result_type.clone(),
        Op::MethodCallExact.default_effects(),
        Some(expr.span),
    );
    let call = ctx.emit_value(
        Op::RuntimeCall,
        vec![raw_call.value, object.value],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(
                crate::ir::RuntimeFnId::DateMagicAppendPropertiesForced,
            ),
        )),
        result_type.clone(),
        crate::ir::RuntimeFnId::DateMagicAppendPropertiesForced.effects(),
        Some(expr.span),
    );
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        &arg_values,
        Some(call.value),
        &ReturnArgAlias::None,
        sig.as_ref(),
        expr.span,
    );
    release_owning_receiver_temporary(ctx, object, expr.span);
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
