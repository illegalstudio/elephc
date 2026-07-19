//! Purpose:
//! Dispatches AOT static syntax and builtin enum/property-hook static methods.
//!
//! Called from:
//! - Static method dispatch after eval-declared targets are exhausted.
//!
//! Key details:
//! - Native receiver metadata, enum backing values, and PHP-visible errors share this boundary.

use super::*;

/// Dispatches one generated/AOT method reached through PHP static-call syntax.
pub(super) fn eval_native_static_syntax_method_result(
    class_name: &str,
    called_class_scope: Option<&str>,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        if eval_native_static_magic_method_available(class_name, context, values)? {
            return eval_native_static_method_with_evaluated_args(
                class_name,
                method_name,
                evaluated_args,
                context,
                values,
            )
            .map(Some);
        }
        return Ok(None);
    };
    if is_abstract {
        return eval_throw_abstract_method_call_error(
            &declaring_class,
            method_name,
            context,
            values,
        );
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        if eval_native_static_magic_method_available(class_name, context, values)? {
            return eval_native_magic_static_method_call(
                class_name,
                method_name,
                evaluated_args,
                context,
                values,
            )
            .map(Some);
        }
        return eval_throw_method_access_error(
            &declaring_class,
            method_name,
            visibility,
            context,
            values,
        );
    }
    if !is_static {
        if let Some(object) =
            eval_static_syntax_instance_receiver(class_name, lexical_scope, context, values)?
        {
            return eval_native_method_with_evaluated_args_bridge_scope(
                object,
                class_name,
                method_name,
                evaluated_args,
                Some(&declaring_class),
                called_class_scope,
                context,
                values,
            )
            .map(Some);
        }
        return eval_throw_non_static_method_call_error(
            &declaring_class,
            method_name,
            context,
            values,
        );
    }
    eval_native_static_method_with_evaluated_args_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        Some(&declaring_class),
        called_class_scope,
        context,
        values,
    )
    .map(Some)
}

/// Returns `$this` when PHP permits static-call syntax to target an instance method.
pub(in crate::interpreter) fn eval_static_syntax_instance_receiver(
    class_name: &str,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(scope) = lexical_scope else {
        return Ok(None);
    };
    let Some(object) = visible_scope_cell(context, scope, "this") else {
        return Ok(None);
    };
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(None);
    }
    let object_class_name = eval_static_syntax_object_class_name(object, context, values)?;
    if eval_static_syntax_object_matches_class(&object_class_name, class_name, context) {
        Ok(Some(object))
    } else {
        Ok(None)
    }
}

/// Resolves the PHP-visible class name for the current static-syntax `$this` object.
pub(super) fn eval_static_syntax_object_class_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class) = context.dynamic_object_class(identity) {
            return Ok(class.name().to_string());
        }
    }
    runtime_object_class_name(object, values)
}

/// Returns whether `$this` is an instance of the class named by static-call syntax.
pub(super) fn eval_static_syntax_object_matches_class(
    object_class_name: &str,
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    same_eval_class_name(object_class_name, class_name)
        || context.class_is_a(object_class_name, class_name, false)
        || native_class_is_a(object_class_name, class_name, context)
}

/// Dispatches static methods for eval's builtin `PropertyHookType` enum slice.
pub(super) fn eval_builtin_property_hook_type_static_method_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("PropertyHookType")
    {
        return Ok(None);
    }
    match eval_enum_static_builtin_name(method_name) {
        Some("cases") => {
            eval_builtin_property_hook_type_cases(evaluated_args, context, values).map(Some)
        }
        Some("from") => {
            eval_builtin_property_hook_type_from(evaluated_args, false, context, values).map(Some)
        }
        Some("tryFrom") => {
            eval_builtin_property_hook_type_from(evaluated_args, true, context, values).map(Some)
        }
        _ => Ok(None),
    }
}

/// Builds the indexed case array for eval's builtin `PropertyHookType` enum slice.
pub(super) fn eval_builtin_property_hook_type_cases(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let case_names = ["Get", "Set"];
    let mut array = values.array_new(case_names.len())?;
    for (index, case_name) in case_names.iter().enumerate() {
        let key = values.int(index as i64)?;
        let case =
            eval_builtin_property_hook_type_case("PropertyHookType", case_name, context, values)?
                .ok_or(EvalStatus::RuntimeFatal)?;
        array = values.array_set(array, key, case)?;
    }
    Ok(array)
}

/// Evaluates builtin `PropertyHookType::from()` or `tryFrom()` inside eval.
pub(super) fn eval_builtin_property_hook_type_from(
    evaluated_args: Vec<EvaluatedCallArg>,
    nullable_miss: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut args = bind_evaluated_function_args(&[String::from("value")], evaluated_args)?;
    let value = args.pop().ok_or(EvalStatus::RuntimeFatal)?;
    let bytes = values.string_bytes(value)?;
    let value_text = String::from_utf8_lossy(&bytes);
    for constant_name in ["Get", "Set"] {
        let Some((_, case_value)) = eval_property_hook_type_case_parts(constant_name) else {
            continue;
        };
        if value_text == case_value {
            return eval_builtin_property_hook_type_case(
                "PropertyHookType",
                constant_name,
                context,
                values,
            )?
            .ok_or(EvalStatus::RuntimeFatal);
        }
    }
    if nullable_miss {
        values.null()
    } else {
        let message = eval_enum_invalid_backing_value_message(
            "PropertyHookType",
            EvalEnumBackingType::String,
            value,
            values,
        )?;
        eval_throw_value_error(&message, context, values)
    }
}

/// Returns a recognized enum-provided static method name.
pub(super) fn eval_enum_static_builtin_name(method_name: &str) -> Option<&'static str> {
    if method_name.eq_ignore_ascii_case("cases") {
        Some("cases")
    } else if method_name.eq_ignore_ascii_case("from") {
        Some("from")
    } else if method_name.eq_ignore_ascii_case("tryFrom") {
        Some("tryFrom")
    } else {
        None
    }
}

/// Returns a synthetic enum method only when that enum actually provides it.
pub(in crate::interpreter) fn eval_enum_static_builtin_applies(
    enum_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<&'static str> {
    let enum_decl = context.enum_decl(enum_name)?;
    match eval_enum_static_builtin_name(method_name)? {
        "cases" => Some("cases"),
        "from" if enum_decl.backing_type().is_some() => Some("from"),
        "tryFrom" if enum_decl.backing_type().is_some() => Some("tryFrom"),
        _ => None,
    }
}

/// Dispatches enum-provided static methods for eval-declared enums.
pub(in crate::interpreter) fn eval_enum_builtin_static_method_result(
    enum_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_enum_static_builtin_name(method_name).ok_or(EvalStatus::RuntimeFatal)? {
        "cases" => eval_enum_cases_result(enum_name, evaluated_args, context, values),
        "from" => eval_enum_from_result(enum_name, evaluated_args, false, context, values),
        "tryFrom" => eval_enum_from_result(enum_name, evaluated_args, true, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds the indexed array returned by `EnumName::cases()`.
pub(super) fn eval_enum_cases_result(
    enum_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let enum_decl = context
        .enum_decl(enum_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let case_names = enum_decl
        .cases()
        .iter()
        .map(|case| case.name().to_string())
        .collect::<Vec<_>>();
    let mut array = values.array_new(case_names.len())?;
    for (index, case_name) in case_names.iter().enumerate() {
        let key = values.int(index as i64)?;
        let case = context
            .enum_case(enum_name, case_name)
            .ok_or(EvalStatus::RuntimeFatal)?;
        array = values.array_set(array, key, case)?;
    }
    Ok(array)
}

/// Evaluates `EnumName::from()` or `EnumName::tryFrom()` for eval-backed enums.
pub(super) fn eval_enum_from_result(
    enum_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    nullable_miss: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let enum_decl = context
        .enum_decl(enum_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let backing_type = enum_decl.backing_type().ok_or(EvalStatus::RuntimeFatal)?;
    let enum_display_name = enum_decl.name().trim_start_matches('\\').to_string();
    let case_names = enum_decl
        .cases()
        .iter()
        .map(|case| case.name().to_string())
        .collect::<Vec<_>>();
    let mut args = bind_evaluated_function_args(&[String::from("value")], evaluated_args)?;
    let value = args.pop().ok_or(EvalStatus::RuntimeFatal)?;
    for case_name in case_names {
        let case_value = context
            .enum_case_value(enum_name, &case_name)
            .ok_or(EvalStatus::RuntimeFatal)?;
        let equal = values.compare(EvalBinOp::StrictEq, value, case_value)?;
        if values.truthy(equal)? {
            return context
                .enum_case(enum_name, &case_name)
                .ok_or(EvalStatus::RuntimeFatal);
        }
    }
    if nullable_miss {
        values.null()
    } else {
        let message = eval_enum_invalid_backing_value_message(
            &enum_display_name,
            backing_type,
            value,
            values,
        )?;
        eval_throw_value_error(&message, context, values)
    }
}

/// Builds PHP's backed-enum `ValueError` message for an unmatched enum value.
pub(super) fn eval_enum_invalid_backing_value_message(
    enum_name: &str,
    backing_type: EvalEnumBackingType,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let value = String::from_utf8_lossy(&bytes);
    let value = match backing_type {
        EvalEnumBackingType::Int => value.into_owned(),
        EvalEnumBackingType::String => format!("\"{}\"", value),
    };
    Ok(format!(
        "{} is not a valid backing value for enum {}",
        value, enum_name
    ))
}

/// Creates and schedules a `ValueError` through eval's normal Throwable channel.
pub(super) fn eval_throw_value_error(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Creates and schedules a `ReflectionException` through eval's normal Throwable channel.
pub(super) fn eval_throw_reflection_exception(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let exception = values.new_object("ReflectionException")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Schedules the Throwable category required by one ReflectionClass instantiation error.
pub(super) fn eval_throw_reflection_instantiation_error(
    error: EvalReflectionInstantiationError,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match error {
        EvalReflectionInstantiationError::ThrowableError(message) => {
            eval_throw_error(&message, context, values)
        }
        EvalReflectionInstantiationError::ReflectionException(message) => {
            eval_throw_reflection_exception(&message, context, values)
        }
    }
}
