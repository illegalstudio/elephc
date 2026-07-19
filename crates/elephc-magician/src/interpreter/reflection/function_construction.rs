//! Purpose:
//! Constructs `ReflectionFunction` owners for eval, closure, and native targets.
//!
//! Called from:
//! - `crate::interpreter::reflection::eval_reflection_owner_new_object()`.
//!
//! Key details:
//! - Closure targets and native parameter/default metadata are attached once here.

use super::*;

/// Builds an eval-backed `ReflectionFunction` object for eval or registered native functions.
pub(super) fn eval_reflection_function_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("function")], evaluated_args)?;
    let closure_target = eval_reflection_function_closure_target_arg(args[0], context, values)?;
    let requested_name = match closure_target.as_ref() {
        Some(target) => eval_reflection_function_closure_target_name(target),
        None => eval_reflection_function_name_arg(args[0], context, values)?,
    };
    let lookup_name = requested_name.trim_start_matches('\\').to_ascii_lowercase();
    if let Some(closure) = context.closure(&requested_name).cloned() {
        let function = closure.function();
        let required_parameter_count = eval_reflection_required_parameter_count(
            function.parameter_defaults(),
            function.parameter_is_variadic(),
        );
        let parameters = eval_reflection_function_parameters(
            function.name(),
            function.params(),
            function.attributes().to_vec(),
            function.parameter_attributes(),
            function.parameter_types(),
            function.parameter_defaults(),
            function.parameter_is_by_ref(),
            function.parameter_is_variadic(),
        );
        return eval_reflection_function_object_result(
            &requested_name,
            function.attributes(),
            &parameters,
            required_parameter_count,
            context,
            values,
        )
        .and_then(|object| {
            eval_reflection_attach_function_closure_target(
                object,
                closure_target,
                context,
                values,
            )
        })
        .map(Some);
    }
    if let Some(function) = context.function(&lookup_name).cloned() {
        let required_parameter_count = eval_reflection_required_parameter_count(
            function.parameter_defaults(),
            function.parameter_is_variadic(),
        );
        let parameters = eval_reflection_function_parameters(
            function.name(),
            function.params(),
            function.attributes().to_vec(),
            function.parameter_attributes(),
            function.parameter_types(),
            function.parameter_defaults(),
            function.parameter_is_by_ref(),
            function.parameter_is_variadic(),
        );
        return eval_reflection_function_object_result(
            function.name(),
            function.attributes(),
            &parameters,
            required_parameter_count,
            context,
            values,
        )
        .and_then(|object| {
            eval_reflection_attach_function_closure_target(
                object,
                closure_target,
                context,
                values,
            )
        })
        .map(Some);
    }
    if let Some(function) = context.native_function(&lookup_name) {
        let reflected_name = requested_name.trim_start_matches('\\');
        let required_parameter_count = function.required_param_count();
        let parameters = eval_reflection_native_function_parameters(reflected_name, &function);
        return eval_reflection_function_object_result(
            reflected_name,
            &[],
            &parameters,
            required_parameter_count,
            context,
            values,
        )
        .and_then(|object| {
            eval_reflection_attach_function_closure_target(
                object,
                closure_target,
                context,
                values,
            )
        })
        .map(Some);
    }
    if closure_target.is_some() {
        return eval_reflection_function_object_result(
            &requested_name,
            &[],
            &[],
            0,
            context,
            values,
        )
        .and_then(|object| {
            eval_reflection_attach_function_closure_target(
                object,
                closure_target,
                context,
                values,
            )
        })
        .map(Some);
    }
    Ok(None)
}

/// Returns the retained callable target when a ReflectionFunction argument is a Closure object.
pub(super) fn eval_reflection_function_closure_target_arg(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalClosureObjectTarget>, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_OBJECT {
        return Ok(None);
    }
    let identity = values.object_identity(value)?;
    Ok(context.closure_object_target(identity).cloned())
}

/// Returns the function-like name exposed for a Closure-backed ReflectionFunction.
pub(super) fn eval_reflection_function_closure_target_name(target: &EvalClosureObjectTarget) -> String {
    match target {
        EvalClosureObjectTarget::Named(name)
        | EvalClosureObjectTarget::BoundNamed { name, .. } => name.clone(),
        EvalClosureObjectTarget::InvokableObject { .. } => String::from("__invoke"),
        EvalClosureObjectTarget::ObjectMethod { method, .. }
        | EvalClosureObjectTarget::StaticMethod { method, .. } => method.clone(),
    }
}

/// Attaches original Closure target metadata to a synthetic ReflectionFunction object.
pub(super) fn eval_reflection_attach_function_closure_target(
    object: RuntimeCellHandle,
    closure_target: Option<EvalClosureObjectTarget>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(closure_target) = closure_target else {
        return Ok(object);
    };
    let identity = values.object_identity(object)?;
    context.register_eval_reflection_function_closure_target(identity, closure_target);
    Ok(object)
}

/// Returns parameter names for a registered native function, filling missing bridge names.
pub(super) fn eval_reflection_native_function_parameter_names(function: &NativeFunction) -> Vec<String> {
    (0..function.param_count())
        .map(|index| {
            function
                .param_names()
                .get(index)
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("arg{}", index))
        })
        .collect()
}

/// Builds ReflectionParameter metadata for one registered native AOT function.
pub(super) fn eval_reflection_native_function_parameters(
    function_name: &str,
    function: &NativeFunction,
) -> Vec<EvalReflectionParameterMetadata> {
    let parameter_names = eval_reflection_native_function_parameter_names(function);
    let parameter_count = parameter_names.len();
    let parameter_attributes = vec![Vec::new(); parameter_count];
    let parameter_types = eval_reflection_native_function_parameter_types(function);
    let parameter_defaults = eval_reflection_native_function_parameter_defaults(function);
    let parameter_is_by_ref = (0..parameter_count)
        .map(|index| function.param_by_ref(index))
        .collect::<Vec<_>>();
    let parameter_is_variadic = (0..parameter_count)
        .map(|index| function.param_variadic(index))
        .collect::<Vec<_>>();
    eval_reflection_function_parameters(
        function_name,
        &parameter_names,
        Vec::new(),
        &parameter_attributes,
        &parameter_types,
        &parameter_defaults,
        &parameter_is_by_ref,
        &parameter_is_variadic,
    )
}

/// Converts registered native function parameter types into reflection metadata input.
pub(super) fn eval_reflection_native_function_parameter_types(
    function: &NativeFunction,
) -> Vec<Option<EvalParameterType>> {
    (0..function.param_count())
        .map(|index| function.param_type(index).cloned())
        .collect()
}

/// Converts registered native function defaults into eval constant expressions.
pub(super) fn eval_reflection_native_function_parameter_defaults(
    function: &NativeFunction,
) -> Vec<Option<EvalExpr>> {
    (0..function.param_count())
        .map(|index| {
            function
                .param_default(index)
                .map(eval_reflection_native_callable_default_expr)
        })
        .collect()
}

/// Builds one `ReflectionFunction` object from retained eval function metadata.
pub(super) fn eval_reflection_function_object_result(
    function_name: &str,
    attributes: &[EvalAttribute],
    parameters: &[EvalReflectionParameterMetadata],
    required_parameter_count: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_FUNCTION,
        function_name,
        attributes,
        &[],
        &[],
        &[],
        &[],
        None,
        parameters,
        None,
        None,
        None,
        None,
        eval_reflection_callable_flags(attributes),
        required_parameter_count as u64,
        0,
        None,
        None,
        context,
        values,
    )
}
