//! Purpose:
//! Implements callable, parameter, and type Reflection method dispatch.
//!
//! Called from:
//! - `crate::interpreter::statements` for ReflectionFunctionAbstract owners.
//!
//! Key details:
//! - Invocation, metadata predicates, prototypes, and PHP string forms share one target lookup.

use super::*;

/// Handles eval-backed `ReflectionMethod::invoke()` and `invokeArgs()` calls.
pub(in crate::interpreter) fn eval_reflection_method_invoke_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_invoke = method_name.eq_ignore_ascii_case("invoke");
    let is_invoke_args = method_name.eq_ignore_ascii_case("invokeArgs");
    if !is_invoke && !is_invoke_args {
        return Ok(None);
    }
    let Some((declaring_class, reflected_method)) = context
        .eval_reflection_method(identity)
        .map(|(declaring_class, method)| (declaring_class.to_string(), method.to_string()))
    else {
        return Ok(None);
    };
    let (object, method_args) = if is_invoke {
        eval_reflection_method_invoke_args(evaluated_args)?
    } else {
        eval_reflection_method_invoke_args_array(evaluated_args, context, values)?
    };
    eval_reflection_method_invoke_dispatch(
        &declaring_class,
        &reflected_method,
        object,
        method_args,
        context,
        values,
    )
    .map(Some)
}

/// Handles eval-backed `ReflectionFunction::invoke()` and `invokeArgs()` calls.
pub(in crate::interpreter) fn eval_reflection_function_invoke_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_invoke = method_name.eq_ignore_ascii_case("invoke");
    let is_invoke_args = method_name.eq_ignore_ascii_case("invokeArgs");
    if !is_invoke && !is_invoke_args {
        return Ok(None);
    }
    let Some(function_name) = context
        .eval_reflection_function_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let function_args = if is_invoke {
        evaluated_args
            .into_iter()
            .map(eval_reflection_method_forwarded_value_arg)
            .collect()
    } else {
        eval_reflection_function_invoke_args_array(evaluated_args, context, values)?
    };
    eval_reflection_function_invoke_dispatch(&function_name, function_args, context, values)
        .map(Some)
}

/// Handles eval-backed ReflectionFunctionAbstract name/origin metadata calls.
pub(in crate::interpreter) fn eval_reflection_function_method_metadata_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(target) = eval_reflection_function_method_target(identity, context, values)? else {
        return Ok(None);
    };
    let method_key = method_name.to_ascii_lowercase();
    match method_key.as_str() {
        "getshortname" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .string(&eval_reflection_function_method_short_name(&target))
                .map(Some)
        }
        "getnamespacename" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .string(&eval_reflection_function_method_namespace_name(&target))
                .map(Some)
        }
        "innamespace" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(!eval_reflection_function_method_namespace_name(&target).is_empty())
                .map(Some)
        }
        "getfilename" | "getstartline" | "getendline" => {
            let (source_file, source_location) =
                eval_reflection_function_method_source_location(&target);
            eval_reflection_source_location_result(
                method_key.as_str(),
                source_file,
                source_location,
                evaluated_args,
                context,
                values,
            )
        }
        "isinternal" | "returnsreference" | "isgenerator" | "hastentativereturntype" => {
            eval_reflection_false_metadata_result(evaluated_args, values)
        }
        "isclosure" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(eval_reflection_function_method_is_closure(&target))
                .map(Some)
        }
        "isdeprecated" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(eval_reflection_function_method_is_deprecated(&target))
                .map(Some)
        }
        "isanonymous" => match target {
            EvalReflectionFunctionMethodTarget::Function { .. } => {
                eval_reflection_bind_no_args(evaluated_args)?;
                values
                    .bool_value(eval_reflection_function_method_is_closure(&target))
                    .map(Some)
            }
            EvalReflectionFunctionMethodTarget::Method { .. } => Ok(None),
        },
        "hasreturntype" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(eval_reflection_function_method_return_type(&target).is_some())
                .map(Some)
        }
        "isuserdefined" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values.bool_value(true).map(Some)
        }
        "isvariadic" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(eval_reflection_function_method_is_variadic(&target))
                .map(Some)
        }
        "isstatic" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(eval_reflection_function_method_is_static(&target))
                .map(Some)
        }
        "isdisabled" => match target {
            EvalReflectionFunctionMethodTarget::Function { .. } => {
                eval_reflection_false_metadata_result(evaluated_args, values)
            }
            EvalReflectionFunctionMethodTarget::Method { .. } => Ok(None),
        },
        "getreturntype" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            match eval_reflection_function_method_return_type(&target) {
                Some(type_metadata) => {
                    eval_reflection_type_object_result(type_metadata, values).map(Some)
                }
                None => values.null().map(Some),
            }
        }
        "gettentativereturntype" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values.null().map(Some)
        }
        "getstaticvariables" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_function_method_static_variables_result(&target, context, values)
                .map(Some)
        }
        "getclosureusedvariables" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_function_closure_used_variables_result(&target, values).map(Some)
        }
        "getclosurethis" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_function_closure_this_result(&target, values).map(Some)
        }
        "getclosurescopeclass" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_function_closure_scope_class_result(&target, context, values)
                .map(Some)
        }
        "getclosurecalledclass" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_function_closure_called_class_result(&target, context, values)
                .map(Some)
        }
        _ => Ok(None),
    }
}

/// Handles eval-backed `ReflectionFunction::__toString()` and `ReflectionMethod::__toString()`.
pub(in crate::interpreter) fn eval_reflection_function_method_to_string_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
        return Ok(None);
    }
    let Some(target) = eval_reflection_function_method_target(identity, context, values)? else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    let rendered = eval_reflection_function_method_to_string(&target);
    values.string(&rendered).map(Some)
}

/// Handles eval-backed `ReflectionParameter::isArray()` and `isCallable()` calls.
pub(in crate::interpreter) fn eval_reflection_parameter_legacy_type_predicate_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(expected_type) = eval_reflection_parameter_legacy_type_name(method_name) else {
        return Ok(None);
    };
    if !eval_reflection_object_has_class(object, "ReflectionParameter", values)? {
        return Ok(None);
    }
    eval_reflection_bind_no_args(evaluated_args)?;
    let type_value = values.method_call(object, "getType", Vec::new())?;
    if values.is_null(type_value)? {
        return values.bool_value(false).map(Some);
    }
    if !eval_reflection_object_has_class(type_value, "ReflectionNamedType", values)? {
        return values.bool_value(false).map(Some);
    }
    let name = values.method_call(type_value, "getName", Vec::new())?;
    let bytes = values.string_bytes(name)?;
    let name = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values
        .bool_value(name.eq_ignore_ascii_case(expected_type))
        .map(Some)
}

/// Handles eval-backed `ReflectionParameter::__toString()` calls.
pub(in crate::interpreter) fn eval_reflection_parameter_to_string_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
        return Ok(None);
    }
    if !eval_reflection_object_has_class(object, "ReflectionParameter", values)? {
        return Ok(None);
    }
    eval_reflection_bind_no_args(evaluated_args)?;
    let rendered = eval_reflection_parameter_object_to_string(object, values)?;
    values.string(&rendered).map(Some)
}

/// Handles eval-backed `ReflectionType::__toString()` calls.
pub(in crate::interpreter) fn eval_reflection_type_to_string_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
        return Ok(None);
    }
    let Some(rendered) = eval_reflection_type_object_to_string(object, values)? else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    values.string(&rendered).map(Some)
}

/// Formats one ReflectionParameter object through its public metadata methods.
pub(super) fn eval_reflection_parameter_object_to_string(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let position = eval_reflection_no_arg_int_method(object, "getPosition", values)?;
    let name = eval_reflection_no_arg_string_method(object, "getName", values)?;
    let is_optional = eval_reflection_no_arg_bool_method(object, "isOptional", values)?;
    let is_passed_by_reference =
        eval_reflection_no_arg_bool_method(object, "isPassedByReference", values)?;
    let is_variadic = eval_reflection_no_arg_bool_method(object, "isVariadic", values)?;
    let type_value = values.method_call(object, "getType", Vec::new())?;
    let type_text = if values.is_null(type_value)? {
        None
    } else {
        eval_reflection_type_object_to_string(type_value, values)?
    };

    let mut signature_parts = Vec::new();
    if let Some(type_text) = type_text {
        signature_parts.push(type_text);
    }
    let mut variable = String::new();
    if is_passed_by_reference {
        variable.push('&');
    }
    if is_variadic {
        variable.push_str("...");
    }
    variable.push('$');
    variable.push_str(&name);
    signature_parts.push(variable);
    let requiredness = if is_optional { "optional" } else { "required" };
    let default = eval_reflection_parameter_object_default_to_string(object, values)?
        .map(|value| format!(" = {value}"))
        .unwrap_or_default();

    Ok(format!(
        "Parameter #{} [ <{}> {}{} ]",
        position,
        requiredness,
        signature_parts.join(" "),
        default
    ))
}

/// Formats a ReflectionParameter default through its public metadata methods.
pub(super) fn eval_reflection_parameter_object_default_to_string(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    if !eval_reflection_no_arg_bool_method(object, "isDefaultValueAvailable", values)? {
        return Ok(None);
    }
    if eval_reflection_no_arg_bool_method(object, "isDefaultValueConstant", values)? {
        let constant_name =
            eval_reflection_no_arg_string_method(object, "getDefaultValueConstantName", values)?;
        if !constant_name.is_empty() {
            return Ok(Some(constant_name));
        }
    }
    let default_value = values.method_call(object, "getDefaultValue", Vec::new())?;
    eval_reflection_runtime_default_value_to_string(default_value, values).map(Some)
}

/// Formats one materialized scalar-ish default value for reflection string output.
pub(super) fn eval_reflection_runtime_default_value_to_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    Ok(match values.type_tag(value)? {
        EVAL_TAG_NULL => String::from("NULL"),
        EVAL_TAG_BOOL => {
            if values.truthy(value)? {
                String::from("true")
            } else {
                String::from("false")
            }
        }
        EVAL_TAG_INT | EVAL_TAG_FLOAT => String::from_utf8_lossy(&values.string_bytes(value)?)
            .into_owned(),
        EVAL_TAG_STRING => {
            let value = String::from_utf8_lossy(&values.string_bytes(value)?).into_owned();
            format!("'{value}'")
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC if values.array_len(value)? == 0 => String::from("[]"),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => String::from("Array"),
        EVAL_TAG_OBJECT => String::from("Object"),
        _ => String::from_utf8_lossy(&values.string_bytes(value)?).into_owned(),
    })
}

/// Calls one no-arg Reflection method and returns its string result.
pub(super) fn eval_reflection_no_arg_string_method(
    object: RuntimeCellHandle,
    method: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let value = values.method_call(object, method, Vec::new())?;
    eval_reflection_string_arg(value, values)
}

/// Calls one no-arg Reflection method and returns its bool result.
pub(super) fn eval_reflection_no_arg_bool_method(
    object: RuntimeCellHandle,
    method: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let value = values.method_call(object, method, Vec::new())?;
    if values.type_tag(value)? != EVAL_TAG_BOOL {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.truthy(value)
}

/// Calls one no-arg Reflection method and returns its int result.
pub(super) fn eval_reflection_no_arg_int_method(
    object: RuntimeCellHandle,
    method: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let value = values.method_call(object, method, Vec::new())?;
    eval_int_value(value, values)
}

/// Formats one eval-visible ReflectionType object if the value is a retained type object.
pub(super) fn eval_reflection_type_object_to_string(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let type_kind = if eval_reflection_object_has_class(object, "ReflectionNamedType", values)? {
        Some(("ReflectionNamedType", ""))
    } else if eval_reflection_object_has_class(object, "ReflectionUnionType", values)? {
        Some(("ReflectionUnionType", "|"))
    } else if eval_reflection_object_has_class(object, "ReflectionIntersectionType", values)? {
        Some(("ReflectionIntersectionType", "&"))
    } else {
        None
    };
    let Some((class_name, separator)) = type_kind else {
        return Ok(None);
    };
    let rendered = if class_name == "ReflectionNamedType" {
        eval_reflection_named_type_to_string(object, values)?
    } else {
        eval_reflection_composite_type_to_string(object, separator, values)?
    };
    Ok(Some(rendered))
}

/// Formats one eval-visible ReflectionNamedType object from its public methods.
pub(super) fn eval_reflection_named_type_to_string(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = eval_reflection_type_method_string(object, "getName", values)?;
    let allows_null = eval_reflection_type_method_bool(object, "allowsNull", values)?;
    if allows_null && name != "mixed" {
        Ok(format!("?{name}"))
    } else {
        Ok(name)
    }
}

/// Formats one eval-visible ReflectionUnionType or ReflectionIntersectionType object.
pub(super) fn eval_reflection_composite_type_to_string(
    object: RuntimeCellHandle,
    separator: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let types = values.method_call(object, "getTypes", Vec::new())?;
    let mut names = Vec::new();
    for position in 0..values.array_len(types)? {
        let key = values.array_iter_key(types, position)?;
        let member = values.array_get(types, key)?;
        names.push(eval_reflection_type_method_string(member, "getName", values)?);
    }
    if separator == "|" && eval_reflection_type_method_bool(object, "allowsNull", values)? {
        names.push(String::from("null"));
    }
    Ok(names.join(separator))
}

/// Calls one no-arg ReflectionType method and returns its string result.
pub(super) fn eval_reflection_type_method_string(
    object: RuntimeCellHandle,
    method: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let value = values.method_call(object, method, Vec::new())?;
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Calls one no-arg ReflectionType method and returns its bool result.
pub(super) fn eval_reflection_type_method_bool(
    object: RuntimeCellHandle,
    method: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let value = values.method_call(object, method, Vec::new())?;
    if values.type_tag(value)? != EVAL_TAG_BOOL {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.truthy(value)
}

/// Maps a legacy ReflectionParameter predicate method to its target named type.
pub(super) fn eval_reflection_parameter_legacy_type_name(method_name: &str) -> Option<&'static str> {
    if method_name.eq_ignore_ascii_case("isArray") {
        Some("array")
    } else if method_name.eq_ignore_ascii_case("isCallable") {
        Some("callable")
    } else {
        None
    }
}

/// Returns whether one runtime object cell has the requested PHP class name.
pub(super) fn eval_reflection_object_has_class(
    object: RuntimeCellHandle,
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    let actual = values.object_class_name(object)?;
    let bytes = values.string_bytes(actual);
    values.release(actual)?;
    let actual = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(actual
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(class_name))
}

/// Handles eval-backed `ReflectionMethod::hasPrototype()` and `getPrototype()` calls.
pub(in crate::interpreter) fn eval_reflection_method_prototype_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_has_prototype = method_name.eq_ignore_ascii_case("hasPrototype");
    let is_get_prototype = method_name.eq_ignore_ascii_case("getPrototype");
    if !is_has_prototype && !is_get_prototype {
        return Ok(None);
    }
    let Some((declaring_class, reflected_method)) =
        context
            .eval_reflection_method(identity)
            .map(|(declaring_class, method_name)| {
                (declaring_class.to_string(), method_name.to_string())
            })
    else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    let Some((prototype_class, prototype_method)) = eval_reflection_method_prototype_target(
        &declaring_class,
        &reflected_method,
        context,
        values,
    )?
    else {
        if is_has_prototype {
            return values.bool_value(false).map(Some);
        }
        return eval_throw_reflection_exception(
            &format!(
                "Method {}::{} does not have a prototype",
                declaring_class, reflected_method
            ),
            context,
            values,
        );
    };
    if is_has_prototype {
        return values.bool_value(true).map(Some);
    }
    let Some(metadata) =
        eval_reflection_prototype_method_metadata(&prototype_class, &prototype_method, context, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        &prototype_method,
        &metadata,
        context,
        values,
    )
    .map(Some)
}

/// Handles PHP's no-op `ReflectionMethod/Property::setAccessible()` calls.
pub(in crate::interpreter) fn eval_reflection_set_accessible_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("setAccessible") {
        return Ok(None);
    }
    if context.eval_reflection_method(identity).is_none()
        && context.eval_reflection_property(identity).is_none()
    {
        return Ok(None);
    }
    let _ = bind_evaluated_function_args(&[String::from("accessible")], evaluated_args)?;
    values.null().map(Some)
}
