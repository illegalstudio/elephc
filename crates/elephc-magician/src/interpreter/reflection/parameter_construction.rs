//! Purpose:
//! Resolves ReflectionParameter constructor selectors and materializes parameters.
//!
//! Called from:
//! - `crate::interpreter::reflection::eval_reflection_owner_new_object()`.
//!
//! Key details:
//! - Function and method targets accept PHP-compatible name or position selectors.

use super::*;

/// Builds an eval-backed `ReflectionParameter` object for a function or method parameter.
pub(super) fn eval_reflection_parameter_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("function"), String::from("param")],
        evaluated_args,
    )?;
    let selector = eval_reflection_parameter_selector(args[1], values)?;
    let Some(parameter) =
        eval_reflection_parameter_constructor_metadata(args[0], selector.clone(), context, values)?
    else {
        return eval_reflection_parameter_constructor_error(args[0], &selector, context, values);
    };
    eval_reflection_parameter_object_result(&parameter, context, values).map(Some)
}

/// Throws the PHP constructor error for eval-backed `ReflectionParameter` misses.
pub(super) fn eval_reflection_parameter_constructor_error(
    target: RuntimeCellHandle,
    selector: &EvalReflectionParameterSelector,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.is_array_like(target)? {
        return eval_reflection_method_parameter_constructor_error(target, selector, context, values);
    }
    if values.type_tag(target)? == EVAL_TAG_STRING {
        return eval_reflection_function_parameter_constructor_error(
            target, selector, context, values,
        );
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Throws the PHP constructor error for eval-backed function parameter misses.
pub(super) fn eval_reflection_function_parameter_constructor_error(
    target: RuntimeCellHandle,
    selector: &EvalReflectionParameterSelector,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let requested_name = eval_reflection_string_arg(target, values)?;
    let lookup_name = requested_name.trim_start_matches('\\').to_ascii_lowercase();
    if context.function(&lookup_name).is_some() || context.native_function(&lookup_name).is_some() {
        return eval_reflection_parameter_selector_error(selector, context, values);
    }
    Ok(None)
}

/// Throws the PHP constructor error for eval-backed method parameter misses.
pub(super) fn eval_reflection_method_parameter_constructor_error(
    target: RuntimeCellHandle,
    selector: &EvalReflectionParameterSelector,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.array_len(target)? != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let zero = values.int(0)?;
    let one = values.int(1)?;
    let receiver = values.array_get(target, zero)?;
    let method = values.array_get(target, one)?;
    let method_name = eval_reflection_string_arg(method, values)?;
    let class_name = match values.type_tag(receiver)? {
        EVAL_TAG_OBJECT => eval_reflection_object_class_name(receiver, context, values)?,
        EVAL_TAG_STRING => eval_reflection_string_arg(receiver, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let reflected_name = context
        .resolve_class_like_name(&class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    if eval_reflection_class_like_exists(&reflected_name, context) {
        let Some(reflected_method_name) = eval_reflection_member_name(
            EVAL_REFLECTION_OWNER_METHOD,
            &reflected_name,
            &method_name,
            context,
        ) else {
            return eval_throw_reflection_exception(
                &format!("Method {}::{}() does not exist", reflected_name, method_name),
                context,
                values,
            );
        };
        if eval_reflection_method_metadata(&reflected_name, &reflected_method_name, context)
            .is_some()
        {
            return eval_reflection_parameter_selector_error(selector, context, values);
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_reflection_aot_method_metadata_with_signature_if_exists(
        &reflected_name,
        &method_name,
        context,
        values,
    )?
    .is_some()
    {
        return eval_reflection_parameter_selector_error(selector, context, values);
    }
    Ok(None)
}

/// Throws PHP's selector-specific ReflectionParameter constructor error.
pub(super) fn eval_reflection_parameter_selector_error(
    selector: &EvalReflectionParameterSelector,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match selector {
        EvalReflectionParameterSelector::Name(_) => eval_throw_reflection_exception(
            "The parameter specified by its name could not be found",
            context,
            values,
        ),
        EvalReflectionParameterSelector::Position(position) if *position < 0 => {
            eval_throw_value_error(
                "ReflectionParameter::__construct(): Argument #2 ($param) must be greater than or equal to 0",
                context,
                values,
            )
        }
        EvalReflectionParameterSelector::Position(_) => eval_throw_reflection_exception(
            "The parameter specified by its offset could not be found",
            context,
            values,
        ),
    }
}

/// Resolves `ReflectionParameter` constructor target metadata.
pub(super) fn eval_reflection_parameter_constructor_metadata(
    target: RuntimeCellHandle,
    selector: EvalReflectionParameterSelector,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionParameterMetadata>, EvalStatus> {
    if values.is_array_like(target)? {
        return eval_reflection_method_parameter_metadata(target, selector, context, values);
    }
    if values.type_tag(target)? == EVAL_TAG_STRING {
        return eval_reflection_function_parameter_metadata(target, selector, context, values);
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Builds selected parameter metadata for an eval or native free function.
pub(super) fn eval_reflection_function_parameter_metadata(
    target: RuntimeCellHandle,
    selector: EvalReflectionParameterSelector,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionParameterMetadata>, EvalStatus> {
    let requested_name = eval_reflection_string_arg(target, values)?;
    let lookup_name = requested_name.trim_start_matches('\\').to_ascii_lowercase();
    if let Some(function) = context.function(&lookup_name).cloned() {
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
        return Ok(eval_reflection_parameter_for_selector(parameters, selector));
    }
    if let Some(function) = context.native_function(&lookup_name) {
        let reflected_name = requested_name.trim_start_matches('\\');
        let parameters = eval_reflection_native_function_parameters(reflected_name, &function);
        return Ok(eval_reflection_parameter_for_selector(parameters, selector));
    }
    Ok(None)
}

/// Builds selected parameter metadata for an eval or generated/AOT method target.
pub(super) fn eval_reflection_method_parameter_metadata(
    target: RuntimeCellHandle,
    selector: EvalReflectionParameterSelector,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionParameterMetadata>, EvalStatus> {
    if values.array_len(target)? != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let zero = values.int(0)?;
    let one = values.int(1)?;
    let receiver = values.array_get(target, zero)?;
    let method = values.array_get(target, one)?;
    let method_name = eval_reflection_string_arg(method, values)?;
    let class_name = match values.type_tag(receiver)? {
        EVAL_TAG_OBJECT => eval_reflection_object_class_name(receiver, context, values)?,
        EVAL_TAG_STRING => eval_reflection_string_arg(receiver, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let reflected_name = context
        .resolve_class_like_name(&class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    let member = if eval_reflection_class_like_exists(&reflected_name, context) {
        let reflected_method_name = eval_reflection_member_name(
            EVAL_REFLECTION_OWNER_METHOD,
            &reflected_name,
            &method_name,
            context,
        );
        let Some(reflected_method_name) = reflected_method_name else {
            return Ok(None);
        };
        let Some(method) =
            eval_reflection_method_metadata(&reflected_name, &reflected_method_name, context)
        else {
            return Ok(None);
        };
        method
    } else {
        let Some(member) = eval_reflection_aot_method_metadata_with_signature_if_exists(
            &reflected_name,
            &method_name,
            context,
            values,
        )?
        else {
            return Ok(None);
        };
        member
    };
    Ok(eval_reflection_parameter_for_selector(
        member.parameters,
        selector,
    ))
}

/// Converts a `ReflectionParameter` selector runtime value to a supported selector.
pub(super) fn eval_reflection_parameter_selector(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReflectionParameterSelector, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_STRING => {
            eval_reflection_string_arg(value, values).map(EvalReflectionParameterSelector::Name)
        }
        EVAL_TAG_INT => {
            eval_int_value(value, values).map(EvalReflectionParameterSelector::Position)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Selects a parameter by PHP name or zero-based position.
pub(super) fn eval_reflection_parameter_for_selector(
    parameters: Vec<EvalReflectionParameterMetadata>,
    selector: EvalReflectionParameterSelector,
) -> Option<EvalReflectionParameterMetadata> {
    match selector {
        EvalReflectionParameterSelector::Name(name) => parameters
            .into_iter()
            .find(|parameter| parameter.name == name),
        EvalReflectionParameterSelector::Position(position) if position >= 0 => {
            parameters.into_iter().nth(position as usize)
        }
        EvalReflectionParameterSelector::Position(_) => None,
    }
}
