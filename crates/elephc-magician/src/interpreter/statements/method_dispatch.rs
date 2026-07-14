//! Purpose:
//! Dispatches eval and native instance methods with dynamic visibility and scope.
//!
//! Called from:
//! - Method-call expression evaluation and reflected invocation.
//!
//! Key details:
//! - Private shadows, native bridges, closures, magic methods, and reference args remain ordered.

use super::*;

/// Dispatches a method call to an eval-declared class method or to the runtime hook.
pub(in crate::interpreter) fn eval_method_call_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_method_call_result_with_evaluated_args(
        object,
        method_name,
        positional_args(evaluated_args),
        context,
        values,
    )
}

/// Dispatches an object method call while preserving named-argument metadata for eval methods.
pub(in crate::interpreter) fn eval_method_call_result_with_evaluated_args(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let evaluated_args = positional_evaluated_arg_values(evaluated_args)?;
        return values.method_call(object, method_name, evaluated_args);
    };
    if let Some(target) = context.closure_object_target(identity).cloned() {
        if let Some(result) =
            eval_closure_object_method_result(target, method_name, evaluated_args.clone(), context, values)?
        {
            return Ok(result);
        }
    }
    if let Some(attribute_metadata) = context.eval_reflection_attribute(identity).cloned() {
        if method_name.eq_ignore_ascii_case("newInstance") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return eval_reflection_attribute_new_instance_result(
                attribute_metadata.attribute(),
                context,
                values,
            );
        }
        if method_name.eq_ignore_ascii_case("getArguments") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            let Some(args) = attribute_metadata.attribute().args() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_class_attribute_args_result(args, values);
        }
        if method_name.eq_ignore_ascii_case("getTarget") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return values.int(attribute_metadata.target() as i64);
        }
        if method_name.eq_ignore_ascii_case("isRepeated") {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            return values.bool_value(attribute_metadata.is_repeated());
        }
    }
    if let Some(result) = eval_reflection_parameter_legacy_type_predicate_result(
        object,
        method_name,
        evaluated_args.clone(),
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_parameter_to_string_result(
        object,
        method_name,
        evaluated_args.clone(),
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) =
        eval_reflection_type_to_string_result(object, method_name, evaluated_args.clone(), values)?
    {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_implements_interface_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_is_subclass_of_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_is_instance_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_source_location_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_basic_metadata_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_method_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_property_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_has_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_enum_methods_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_relation_objects_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_trait_aliases_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_constants_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_default_properties_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_static_properties_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_static_property_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_set_static_property_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_invoke_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_invoke_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_method_metadata_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_function_method_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_prototype_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_set_accessible_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_hooks_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_is_initialized_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_lazy_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_constant_to_string_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_enum_case_get_enum_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_get_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_raw_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_property_set_value_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_reflection_constant_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_reflection_constants_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_members_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_class_get_member_result(
        object,
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(instance) = eval_reflection_class_new_instance_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(instance);
    }
    if let Some(instance) = eval_reflection_class_new_instance_without_constructor_result(
        identity,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(instance);
    }
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        if method_name.eq_ignore_ascii_case("__clone") {
            if let Some((declaring_class, visibility, is_static, is_abstract)) =
                eval_aot_method_dispatch_metadata(&class_name, method_name, values)?
            {
                if is_static || is_abstract {
                    return Err(EvalStatus::RuntimeFatal);
                }
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return eval_throw_method_access_error(
                        &declaring_class,
                        method_name,
                        visibility,
                        context,
                        values,
                    );
                }
            }
        }
        return eval_native_method_with_evaluated_args(
            object,
            &class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    if eval_enum_static_builtin_applies(&called_class_name, method_name, context).is_some() {
        return eval_enum_builtin_static_method_result(
            &called_class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    let mut inaccessible_method = None;
    if let Some((class_name, method)) =
        eval_dynamic_method_for_call(&called_class_name, method_name, context)
    {
        if method.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if validate_eval_member_access(&class_name, method.visibility(), context).is_ok() {
            if method.is_static() {
                return eval_dynamic_static_method_with_values(
                    &class_name,
                    &called_class_name,
                    &method,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_dynamic_method_with_values(
                &class_name,
                &called_class_name,
                &method,
                object,
                evaluated_args,
                context,
                values,
            );
        }
        inaccessible_method = Some((class_name, method));
    }
    if inaccessible_method.is_none() {
        if let Some(parent) = context.class_native_parent_name(&called_class_name) {
            if let Some((declaring_class, _, _, _)) =
                eval_aot_method_dispatch_metadata_in_hierarchy(
                    &parent,
                    method_name,
                    context,
                    values,
                )?
            {
                return eval_native_method_with_evaluated_args_bridge_scope(
                    object,
                    &parent,
                    method_name,
                    evaluated_args,
                    Some(&declaring_class),
                    Some(&called_class_name),
                    context,
                    values,
                );
            }
            if eval_native_instance_magic_method_available(&parent, context, values)? {
                return eval_native_method_with_evaluated_args(
                    object,
                    &parent,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
        }
    }
    if let Some(result) = eval_magic_instance_method_call(
        object,
        &called_class_name,
        method_name,
        evaluated_args,
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some((declaring_class, method)) = inaccessible_method {
        return eval_throw_method_access_error(
            &declaring_class,
            method.name(),
            method.visibility(),
            context,
            values,
        );
    }
    eval_throw_undefined_method_call_error(&called_class_name, method_name, context, values)
}

/// Dispatches PHP-visible methods on eval-backed `Closure` objects.
pub(super) fn eval_closure_object_method_result(
    target: EvalClosureObjectTarget,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if method_name.eq_ignore_ascii_case("__invoke") {
        return eval_closure_object_invoke_result(target, evaluated_args, context, values)
            .map(Some);
    }
    if method_name.eq_ignore_ascii_case("bindTo") {
        let (bound_this, bound_scope, rebinds_function_scope) =
            eval_closure_bind_to_args(evaluated_args, context, values)?;
        return eval_closure_bind_target(
            target,
            bound_this,
            bound_scope,
            rebinds_function_scope,
            context,
            values,
        )
        .map(Some);
    }
    if !method_name.eq_ignore_ascii_case("call") {
        return Ok(None);
    }
    let (bound_this, call_args) = eval_closure_call_split_args(evaluated_args)?;
    match target {
        EvalClosureObjectTarget::Named(name) => {
            if context.closure(&name).is_some() {
                let callable = EvaluatedCallable::BoundClosure {
                    name,
                    bound_this: Some(bound_this),
                    bound_scope: None,
                };
                return eval_evaluated_callable_with_by_value_call_args(
                    &callable, call_args, context, values,
                )
                .map(Some);
            }
            eval_closure_call_warning_null(
                "Cannot rebind scope of closure created from function",
                values,
            )
            .map(Some)
        }
        EvalClosureObjectTarget::BoundNamed {
            name, bound_scope, ..
        } => {
            if context.closure(&name).is_some() {
                let callable = EvaluatedCallable::BoundClosure {
                    name,
                    bound_this: Some(bound_this),
                    bound_scope,
                };
                return eval_evaluated_callable_with_by_value_call_args(
                    &callable, call_args, context, values,
                )
                .map(Some);
            }
            eval_closure_call_warning_null(
                "Cannot rebind scope of closure created from function",
                values,
            )
            .map(Some)
        }
        EvalClosureObjectTarget::InvokableObject { object } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                )
                .map(Some);
            }
            let callable = EvaluatedCallable::InvokableObject { object: bound_this };
            eval_evaluated_callable_with_by_value_call_args(&callable, call_args, context, values)
                .map(Some)
        }
        EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            called_class: _,
            native_class,
            bridge_scope,
        } => {
            if !eval_closure_call_bound_class_matches(object, bound_this, context, values)? {
                return eval_closure_call_warning_null(
                    "Cannot rebind scope of closure created from method",
                    values,
                )
                .map(Some);
            }
            let called_class = Some(eval_closure_bound_object_class_name(
                bound_this, context, values,
            )?);
            let callable = EvaluatedCallable::ObjectMethod {
                object: bound_this,
                method,
                called_class,
                native_class,
                bridge_scope,
            };
            eval_evaluated_callable_with_by_value_call_args(
                &callable, call_args, context, values,
            )
                .map(Some)
        }
        EvalClosureObjectTarget::StaticMethod { .. } => eval_closure_call_warning_null(
            "Cannot bind an instance to a static closure",
            values,
        )
        .map(Some),
    }
}

/// Invokes the callable target retained behind a PHP-visible eval `Closure` object.
pub(super) fn eval_closure_object_invoke_result(
    target: EvalClosureObjectTarget,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callable = match target {
        EvalClosureObjectTarget::Named(name) => EvaluatedCallable::Named {
            display_name: name.clone(),
            name,
        },
        EvalClosureObjectTarget::BoundNamed {
            name,
            bound_this,
            bound_scope,
        } => EvaluatedCallable::BoundClosure {
            name,
            bound_this,
            bound_scope,
        },
        EvalClosureObjectTarget::InvokableObject { object } => {
            EvaluatedCallable::InvokableObject { object }
        }
        EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        },
        EvalClosureObjectTarget::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvaluatedCallable::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        },
    };
    eval_evaluated_callable_with_call_array_args(&callable, evaluated_args, context, values)
}

/// Splits `Closure::call()` arguments into the bound object and forwarded closure args.
pub(super) fn eval_closure_call_split_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, Vec<EvaluatedCallArg>), EvalStatus> {
    let mut bound_this = None;
    let mut consumed_positional_receiver = false;
    let mut call_args = Vec::with_capacity(evaluated_args.len().saturating_sub(1));

    for arg in evaluated_args {
        if arg
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("newThis"))
        {
            if bound_this.replace(arg.value).is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            continue;
        }
        if arg.name.is_none() && !consumed_positional_receiver && bound_this.is_none() {
            consumed_positional_receiver = true;
            bound_this = Some(arg.value);
            continue;
        }
        call_args.push(arg);
    }

    bound_this
        .map(|receiver| (receiver, call_args))
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns whether `Closure::call()` may bind a method closure to the new object.
pub(super) fn eval_closure_call_bound_class_matches(
    original_object: RuntimeCellHandle,
    bound_this: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let original_class = eval_closure_bound_object_class_name(original_object, context, values)?;
    let bound_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    Ok(original_class.eq_ignore_ascii_case(&bound_class))
}

/// Returns whether `Closure::bind()` may bind a method closure to the new object.
pub(super) fn eval_closure_bind_bound_class_matches_method(
    original_object: RuntimeCellHandle,
    method_name: &str,
    bound_this: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(declaring_class) =
        eval_closure_bind_method_declaring_class(original_object, method_name, context, values)?
    else {
        return Ok(false);
    };
    eval_closure_object_is_instance_of(bound_this, &declaring_class, context, values)
}

/// Resolves the class that declares the method captured by a method Closure target.
pub(super) fn eval_closure_bind_method_declaring_class(
    original_object: RuntimeCellHandle,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let original_class = eval_closure_bound_object_class_name(original_object, context, values)?;
    if let Some((declaring_class, method)) = context.class_method(&original_class, method_name) {
        if method.is_static() || method.is_abstract() {
            return Ok(None);
        }
        return Ok(Some(declaring_class));
    }
    let native_class = context
        .class_native_parent_name(&original_class)
        .unwrap_or_else(|| original_class.clone());
    let Some((_, _, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(&native_class, method_name, context, values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Ok(None);
    }
    let declaring_class = eval_aot_method_declaring_class(&native_class, method_name, values)?;
    Ok(Some(declaring_class))
}

/// Returns whether an object is an instance of the requested eval or generated class name.
pub(super) fn eval_closure_object_is_instance_of(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let object_class = eval_closure_bound_object_class_name(object, context, values)?;
    Ok(eval_static_syntax_object_matches_class(
        &object_class,
        class_name,
        context,
    ))
}

/// Emits PHP's `Closure::call()` warning and returns `null`.
pub(super) fn eval_closure_call_warning_null(
    message: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning(message)?;
    values.null()
}
