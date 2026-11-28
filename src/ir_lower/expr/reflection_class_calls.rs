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
    let args = reflection_class_new_instance_args(args, ctx.semantic_profile);
    let constructor_sig =
        reflection_class_new_instance_constructor_signature(ctx, object_expr, &args).cloned();
    if args.iter().any(is_spread_arg)
        || (crate::types::call_args::has_named_args(&args, ctx.semantic_profile) && constructor_sig.is_none())
    {
        return lower_reflection_class_new_instance_unsupported(ctx, expr);
    }
    let class_name = lower_property_get_from_value(ctx, object, "__name", Op::PropGet, expr);
    let pins = pin_in_flight_owners(ctx, &[class_name.value], expr.span);
    // Dynamic construction owns signature binding, including one variadic collection.
    let result = object_construction::lower_new_dynamic_generic_for_reflection(ctx, class_name, &args, expr);
    unpin_in_flight_owners(ctx, pins, expr.span);
    if ctx.value_is_owning_temporary(class_name) {
        crate::ir_lower::ownership::release_if_owned(ctx, class_name, Some(expr.span));
    }
    result
}

/// Evaluates the argument array before runtime constructor binding can observe its keys.
pub(super) fn lower_reflection_class_new_instance_args(
    ctx: &mut LoweringContext<'_, '_>,
    _object_expr: Option<&Expr>,
    object: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let Some(mut call) = begin_reflection_array_call(
        ctx, object, "ReflectionClass", "newInstanceArgs", args, 0, expr,
    )
    else { return lower_reflection_class_new_instance_args_unsupported(ctx, expr); };
    let reflector = call.reflector(ctx, expr.span);
    let class_name = lower_property_get_from_value(ctx, reflector, "__name", Op::PropGet, expr);
    let class_name = call.root_value(ctx, class_name, expr.span);
    let class_name = lower_expr(ctx, &class_name);
    let unpack = call.unpack(0, expr.span);
    let result = object_construction::lower_new_dynamic_generic_for_reflection(
        ctx,
        class_name,
        &[unpack],
        expr,
    );
    finish_reflection_array_call(ctx, call, result, expr.span)
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
    if let Some(class_name) = object_expr.and_then(|object| reflection_class_reflected_class(ctx, object)) {
        if is_final_internal_class(&class_name) {
            let message = format!(
                "Class {} is an internal class marked as final that cannot be instantiated without invoking its constructor",
                class_name.trim_start_matches('\\')
            );
            let exception = Expr::new(
                ExprKind::NewObject {
                    class_name: Name::unqualified("ReflectionException"),
                    args: vec![Expr::new(ExprKind::StringLiteral(message), expr.span)],
                },
                expr.span,
            );
            let placeholder = lower_null(ctx, expr);
            let exception = lower_expr(ctx, &exception);
            ctx.builder.terminate(Terminator::Throw {
                value: exception.value,
            });
            // A value-context call still lowers the surrounding expression. Continue in an
            // unreachable block so the caller has a well-formed insertion point.
            let unreachable = ctx
                .builder
                .create_named_block("reflection_without_constructor_unreachable", Vec::new());
            ctx.builder.position_at_end(unreachable);
            return placeholder;
        }
    }
    let class_name = lower_property_get_from_value(ctx, object, "__name", Op::PropGet, expr);
    let pins = pin_in_flight_owners(ctx, &[class_name.value], expr.span);
    let result = ctx.emit_value(
        Op::DynamicObjectNewWithoutConstructorMixed,
        vec![class_name.value],
        None,
        PhpType::Mixed,
        Op::DynamicObjectNewWithoutConstructorMixed.default_effects(),
        Some(expr.span),
    );
    unpin_in_flight_owners(ctx, pins, expr.span);
    if ctx.value_is_owning_temporary(class_name) {
        crate::ir_lower::ownership::release_if_owned(ctx, class_name, Some(expr.span));
    }
    result
}

/// Returns whether `class_name` is one of the contract's internal-final classes.
fn is_final_internal_class(class_name: &str) -> bool {
    let key = php_symbol_key(class_name.trim_start_matches('\\'));
    elephc_builtin_contract::INTERNAL_FINAL_CLASSES
        .iter()
        .any(|name| php_symbol_key(name) == key)
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
    let class_name = reflection_class_reflected_class(ctx, object_expr?)?;
    let (member_class, items): (&str, Vec<Expr>) = match php_symbol_key(method).as_str() {
        "getproperties" => {
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
    reflector: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let method_key = php_symbol_key(method);
    let function_name = match object_expr.and_then(|object| reflection_function_reflected_target(ctx, object)) {
        Some(name) => name,
        None => return refuse_unresolved_reflection_call(ctx, reflector, "ReflectionFunction", &method_key, expr),
    };
    let mut array_call = None;
    let Some(forwarded_args) = (match method_key.as_str() {
        "invoke" => Some(reflection_function_invoke_args(args, ctx.semantic_profile)),
        "invokeargs" => begin_reflection_array_call(ctx, reflector, "ReflectionFunction", "invokeArgs", args, 0, expr)
            .map(|call| {
                let forwarded = vec![call.unpack(0, expr.span)];
                array_call = Some(call);
                forwarded
            }),
        _ => return None,
    }) else {
        return Some(lower_reflection_function_invoke_unsupported(
            ctx,
            &method_key,
            expr,
        ));
    };
    let owner = if array_call.is_none() {
        root_method_receiver(ctx, reflector, expr.span).1
    } else { None };
    let result = if let Some(signature) = first_class_builtin_signature(ctx, &function_name) {
        lower_reflection_builtin_function_call(
            ctx,
            &function_name,
            &signature,
            &forwarded_args,
            expr,
        )
    } else {
        let name = Name::from(function_name);
        lower_function_call(ctx, &name, &forwarded_args, expr)
    };
    if let Some(call) = array_call {
        return Some(finish_reflection_array_call(ctx, call, result, expr.span));
    }
    if let Some(owner) = owner {
        super::call_operand_owners::retire_owned_call_operand_preserving_result(ctx, owner, result, expr.span);
    }
    Some(result)
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
pub(super) fn reflection_function_invoke_args(args: &[Expr], profile: elephc_semantics::SemanticProfile) -> Vec<Expr> {
    reflection_class_new_instance_args(args, profile)
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
