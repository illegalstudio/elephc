//! Purpose:
//! ReflectionClass construction and reflected function invocation.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `ReflectionClass::newInstance()` by constructing the reflected class name.
pub(super) fn lower_reflection_class_new_instance(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    object: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let args = reflection_class_new_instance_args(args);
    let constructor_sig =
        reflection_class_new_instance_constructor_signature(ctx, object_expr, &args).cloned();
    if args.iter().any(is_spread_arg)
        || (crate::types::call_args::has_named_args(&args) && constructor_sig.is_none())
    {
        return lower_reflection_class_new_instance_unsupported(ctx, expr);
    }
    let class_name = lower_property_get_from_value(ctx, object, "__name", Op::PropGet, expr);
    let mut operands = vec![class_name.value];
    operands.extend(lower_args_with_signature(
        ctx,
        constructor_sig.as_ref(),
        &args,
    ));
    ctx.emit_value(
        Op::DynamicObjectNewMixed,
        operands,
        None,
        PhpType::Mixed,
        Op::DynamicObjectNewMixed.default_effects(),
        Some(expr.span),
    )
}

/// Lowers `ReflectionClass::newInstanceArgs()` by unpacking one static argument array.
pub(super) fn lower_reflection_class_new_instance_args(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    object: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let Some(forwarded_args) = reflection_class_new_instance_args_array(ctx, args) else {
        return lower_reflection_class_new_instance_args_unsupported(ctx, expr);
    };
    lower_reflection_class_new_instance(ctx, object_expr, object, &forwarded_args, expr)
}

/// Lowers `ReflectionClass::newInstanceWithoutConstructor()` to constructorless allocation.
pub(super) fn lower_reflection_class_new_instance_without_constructor(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    object: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    if !args.is_empty() {
        return lower_reflection_class_new_instance_without_constructor_unsupported(ctx, expr);
    }
    if let Some(class_name) = object_expr
        .and_then(|object_expr| reflection_class_reflected_class(ctx, object_expr))
    {
        release_owning_receiver_temporary(ctx, object, expr.span);
        let class_data = ctx.intern_class_name(&class_name);
        return ctx.emit_value(
            Op::ObjectNewWithoutConstructor,
            Vec::new(),
            Some(Immediate::Data(class_data)),
            PhpType::Object(class_name),
            Op::ObjectNewWithoutConstructor.default_effects(),
            Some(expr.span),
        );
    }
    let class_name = lower_property_get_from_value(ctx, object, "__name", Op::PropGet, expr);
    ctx.emit_value(
        Op::DynamicObjectNewWithoutConstructorMixed,
        vec![class_name.value],
        None,
        PhpType::Mixed,
        Op::DynamicObjectNewWithoutConstructorMixed.default_effects(),
        Some(expr.span),
    )
}

/// Lowers live static-property value access for statically-known `ReflectionClass` calls.
pub(super) fn lower_reflection_class_static_property_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let class_name = reflection_class_reflected_class(ctx, object_expr?)?;
    match php_symbol_key(method).as_str() {
        "getstaticproperties" => {
            lower_reflection_class_get_static_properties(ctx, &class_name, args, expr)
        }
        "getstaticpropertyvalue" => {
            lower_reflection_class_get_static_property_value(ctx, &class_name, args, expr)
        }
        "setstaticpropertyvalue" => {
            lower_reflection_class_set_static_property_value(ctx, &class_name, args, expr)
        }
        _ => None,
    }
}

/// Lowers statically-known filtered ReflectionClass member-list calls.
pub(super) fn lower_reflection_class_member_list_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object_expr = object_expr?;
    let class_name = reflection_class_reflected_class(ctx, object_expr)?;
    let (member_class, items): (&str, Vec<Expr>) = match php_symbol_key(method).as_str() {
        "getproperties" => {
            if reflection_owner_receiver_is_object(ctx, object_expr) {
                return None;
            }
            let filter = reflection_class_get_properties_filter_arg(ctx, args)?;
            (
                "ReflectionProperty",
                reflection_class_property_names_for_filter(ctx, &class_name, filter)?
                    .into_iter()
                    .map(|property| {
                        reflection_member_constructor_expr(
                            "ReflectionProperty",
                            &class_name,
                            &property,
                            expr.span,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "getmethods" => {
            let filter = reflection_class_get_methods_filter_arg(ctx, args)?;
            (
                "ReflectionMethod",
                reflection_class_method_names_for_filter(ctx, &class_name, filter)?
                    .into_iter()
                    .map(|method| {
                        reflection_member_constructor_expr(
                            "ReflectionMethod",
                            &class_name,
                            &method,
                            expr.span,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }
        _ => return None,
    };
    Some(lower_reflection_member_array(
        ctx,
        member_class,
        &items,
        expr,
    ))
}

/// Lowers a statically materialized Reflection member list with an explicit element type.
pub(super) fn lower_reflection_member_array(
    ctx: &mut LoweringContext<'_, '_>,
    member_class: &str,
    items: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let elem_ty = PhpType::Object(member_class.to_string());
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for item in items {
        let value = lower_expr(ctx, item);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        release_value_after_retaining_insert(ctx, Some(&elem_ty), value, item.span);
    }
    array
}

/// Builds a direct Reflection member constructor expression for known metadata.
pub(super) fn reflection_member_constructor_expr(
    reflection_class: &str,
    reflected_class: &str,
    member: &str,
    span: Span,
) -> Expr {
    Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified(reflection_class),
            args: vec![
                Expr::new(ExprKind::StringLiteral(reflected_class.to_string()), span),
                Expr::new(ExprKind::StringLiteral(member.to_string()), span),
            ],
        },
        span,
    )
}

/// Lowers reflected function invocation for statically-known `ReflectionFunction` objects.
pub(super) fn lower_reflection_function_invoke_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let method_key = php_symbol_key(method);
    let object_expr = object_expr?;
    let function_name = reflection_function_reflected_target(ctx, object_expr)?;
    let Some(forwarded_args) = (match method_key.as_str() {
        "invoke" => Some(reflection_function_invoke_args(args)),
        "invokeargs" => reflection_function_invoke_args_array(ctx, args),
        _ => return None,
    }) else {
        return Some(lower_reflection_function_invoke_unsupported(
            ctx,
            &method_key,
            expr,
        ));
    };
    if let Some(signature) = first_class_builtin_signature(&function_name) {
        return Some(lower_reflection_builtin_function_call(
            ctx,
            &function_name,
            &signature,
            &forwarded_args,
            expr,
        ));
    }
    let name = Name::from(function_name);
    Some(lower_function_call(ctx, &name, &forwarded_args, expr))
}

/// Lowers reflected invocation of a supported callable builtin.
pub(super) fn lower_reflection_builtin_function_call(
    ctx: &mut LoweringContext<'_, '_>,
    function_name: &str,
    signature: &FunctionSig,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let operands = lower_builtin_call_args(ctx, function_name, Some(signature), args);
    let php_type = registry_builtin_result_type(ctx, function_name, args, &operands, expr.span)
        .unwrap_or_else(|| call_return_type(ctx, function_name, &operands));
    emit_builtin_call_value(
        ctx,
        function_name,
        operands,
        php_type,
        expr.span,
        None,
    )
}

/// Returns direct `ReflectionFunction::invoke(...$args)` arguments after static spread expansion.
pub(super) fn reflection_function_invoke_args(args: &[Expr]) -> Vec<Expr> {
    reflection_class_new_instance_args(args)
}

/// Extracts the argument list passed to `ReflectionFunction::invokeArgs($args)`.
pub(super) fn reflection_function_invoke_args_array(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [forwarded] => reflection_class_new_instance_args_value(ctx, forwarded),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionFunction")
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
    let forwarded_arg = planned_regular_arg_expr(plan.regular_args.first()?)?;
    reflection_class_new_instance_args_value(ctx, forwarded_arg)
}

/// Emits a runtime fatal for ReflectionFunction invocation forms not yet lowered.
pub(super) fn lower_reflection_function_invoke_unsupported(
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
        "Fatal error: unsupported ReflectionFunction::{}() target or argument forwarding\n",
        method_name
    ));
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}
