//! Purpose:
//! Invokes reflected eval, closure, native, and AOT functions or methods.
//!
//! Called from:
//! - Callable Reflection APIs after PHP argument normalization.
//!
//! Key details:
//! - Forwarded named arguments and declaring-class scope are preserved across dispatch paths.

use super::*;

/// Binds `ReflectionMethod::invoke()` arguments and preserves forwarded named args.
pub(super) fn eval_reflection_method_invoke_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, Vec<EvaluatedCallArg>), EvalStatus> {
    let mut object = None;
    let mut method_args = Vec::new();
    for arg in evaluated_args {
        if matches!(arg.name.as_deref(), Some("object")) {
            if object.is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            object = Some(arg.value);
        } else if object.is_none() && arg.name.is_none() {
            object = Some(arg.value);
        } else {
            method_args.push(eval_reflection_method_forwarded_value_arg(arg));
        }
    }
    object
        .map(|object| (object, method_args))
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Converts a variadic `invoke()` argument into a by-value forwarded method argument.
pub(super) fn eval_reflection_method_forwarded_value_arg(arg: EvaluatedCallArg) -> EvaluatedCallArg {
    EvaluatedCallArg {
        name: arg.name,
        value: arg.value,
        ref_target: None,
    }
}

/// Binds `ReflectionMethod::invokeArgs()` and expands its PHP argument array.
pub(super) fn eval_reflection_method_invoke_args_array(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Vec<EvaluatedCallArg>), EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("object"), String::from("args")],
        evaluated_args,
    )?;
    let method_args = eval_array_call_arg_values(args[1], context, values)?;
    Ok((args[0], method_args))
}

/// Binds `ReflectionFunction::invokeArgs()` and expands its PHP argument array.
pub(super) fn eval_reflection_function_invoke_args_array(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("args")], evaluated_args)?;
    eval_array_call_arg_values(args[0], context, values)
}

/// Dispatches one reflected function invocation through eval or registered native functions.
pub(super) fn eval_reflection_function_invoke_dispatch(
    function_name: &str,
    function_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(closure) = context.closure(function_name).cloned() {
        return eval_closure_with_evaluated_args_and_bound_scope_ref_mode(
            &closure,
            None,
            closure.function().parameter_is_by_ref(),
            function_args,
            EvalByRefBindingMode::WarnByValue {
                callable_name: closure.function().name(),
            },
            context,
            values,
        );
    }
    let function_key = function_name.to_ascii_lowercase();
    if let Some(function) = context.function(&function_key).cloned() {
        return eval_dynamic_function_with_evaluated_args_and_ref_mode(
            &function,
            function.parameter_is_by_ref(),
            function_args,
            EvalByRefBindingMode::WarnByValue {
                callable_name: function.name(),
            },
            context,
            values,
        );
    }
    eval_callable_with_call_array_args(&function_key, function_args, context, values)
}

/// Dispatches one reflected method invocation through eval or AOT bridges.
pub(super) fn eval_reflection_method_invoke_dispatch(
    declaring_class: &str,
    method_name: &str,
    object: RuntimeCellHandle,
    method_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let lookup_method_name = eval_reflection_property_hook_synthetic_method_name(method_name)
        .unwrap_or_else(|| method_name.to_string());
    if let Some((method_class, method)) = context.class_method(declaring_class, &lookup_method_name)
    {
        if method.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let callable_name = format!(
            "{}::{}",
            method_class.trim_start_matches('\\'),
            method.name()
        );
        if method.is_static() {
            return eval_dynamic_static_method_with_values_and_ref_mode(
                &method_class,
                &method_class,
                &method,
                method.parameter_is_by_ref(),
                method_args,
                EvalByRefBindingMode::WarnByValue {
                    callable_name: &callable_name,
                },
                context,
                values,
            );
        }
        let called_class =
            eval_reflection_method_instance_called_class(declaring_class, object, context, values)?;
        return eval_dynamic_method_with_values_and_ref_mode(
            &method_class,
            &called_class,
            &method,
            object,
            method.parameter_is_by_ref(),
            method_args,
            EvalByRefBindingMode::WarnByValue {
                callable_name: &callable_name,
            },
            context,
            values,
        );
    }
    if eval_enum_static_builtin_applies(declaring_class, &lookup_method_name, context).is_some() {
        return eval_enum_builtin_static_method_result(
            declaring_class,
            &lookup_method_name,
            method_args,
            context,
            values,
        );
    }
    eval_reflection_aot_method_invoke_dispatch(
        declaring_class,
        method_name,
        object,
        method_args,
        context,
        values,
    )
}

/// Returns the runtime class name for an eval object used as a reflected receiver.
pub(super) fn eval_reflection_method_instance_called_class(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(object)?;
    let Some(object_class_name) = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if !context.class_is_a(&object_class_name, declaring_class, false) {
        eval_throw_reflection_exception(
            "Given object is not an instance of the class this method was declared in",
            context,
            values,
        )?;
        return Err(EvalStatus::UncaughtThrowable);
    }
    Ok(object_class_name)
}

/// Invokes one reflected generated/AOT method when it fits the bridge slice.
pub(super) fn eval_reflection_aot_method_invoke_dispatch(
    declaring_class: &str,
    method_name: &str,
    object: RuntimeCellHandle,
    method_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let member =
        eval_reflection_aot_method_metadata_if_exists(declaring_class, method_name, values)?
            .ok_or(EvalStatus::RuntimeFatal)?;
    if member.is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if member.is_static {
        return eval_reflection_with_declaring_class_scope(declaring_class, context, |context| {
            eval_native_static_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
                declaring_class,
                method_name,
                method_args,
                Some(declaring_class),
                Some(declaring_class),
                context,
                values,
            )
        });
    }
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let is_instance = dynamic_object_is_a(object, declaring_class, false, context, values)?
        .map_or_else(|| values.object_is_a(object, declaring_class, false), Ok)?;
    if !is_instance {
        eval_throw_reflection_exception(
            "Given object is not an instance of the class this method was declared in",
            context,
            values,
        )?;
        return Err(EvalStatus::UncaughtThrowable);
    }
    let called_class = eval_reflection_object_class_name(object, context, values)?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, |context| {
        eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
            object,
            declaring_class,
            method_name,
            method_args,
            Some(declaring_class),
            Some(&called_class),
            context,
            values,
        )
    })
}

/// Runs a reflected AOT invocation with the declaring class as visibility scope.
pub(super) fn eval_reflection_with_declaring_class_scope<T>(
    declaring_class: &str,
    context: &mut ElephcEvalContext,
    action: impl FnOnce(&mut ElephcEvalContext) -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    context.push_class_scope(declaring_class.to_string());
    context.push_called_class_scope(declaring_class.to_string());
    let result = action(context);
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
}
