//! Purpose:
//! Implements ReflectionClass member and reflected-constant collection APIs.
//!
//! Called from:
//! - `crate::interpreter::statements` for ReflectionClass member dispatch.
//!
//! Key details:
//! - Eval and AOT members share filtering, object construction, and missing-member errors.

use super::*;

/// Handles eval-backed `ReflectionClass::getReflectionConstant()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_reflection_constant_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getReflectionConstant") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let requested_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_constant_names(&reflected_name, context, values)?
        .iter()
        .any(|name| name == &requested_name)
    {
        return values.bool_value(false).map(Some);
    }
    eval_reflection_class_constant_object_result(&reflected_name, &requested_name, context, values)
        .map(Some)
}

/// Handles eval-backed `ReflectionClass::getReflectionConstants()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_reflection_constants_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getReflectionConstants") {
        return Ok(None);
    }
    // The receiver is checked BEFORE the filter argument is read: a user class with its own
    // method of this name reaches here too, and its arguments are not a reflection filter.
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let filter = eval_reflection_member_filter(evaluated_args, values)?;
    let names = eval_reflection_constant_names(&reflected_name, context, values)?;
    let mut result = values.array_new(names.len())?;
    let mut index = 0;
    for name in &names {
        if !eval_reflection_constant_matches_filter(reflected_name.as_str(), name, filter, context, values)?
        {
            continue;
        }
        let object =
            eval_reflection_class_constant_object_result(&reflected_name, name, context, values)?;
        let key = values.int(index)?;
        result = values.array_set(result, key, object)?;
        index += 1;
    }
    Ok(Some(result))
}

/// Handles eval-backed `ReflectionClass::getMethods()` and `getProperties()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_members_result(
    object: RuntimeCellHandle,
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let owner_kind = if method_name.eq_ignore_ascii_case("getMethods") {
        EVAL_REFLECTION_OWNER_METHOD
    } else if method_name.eq_ignore_ascii_case("getProperties") {
        EVAL_REFLECTION_OWNER_PROPERTY
    } else {
        return Ok(None);
    };
    // The receiver is checked BEFORE the filter argument is read: Symfony UX's
    // `ComponentProperties::getProperties(object $component, bool $publicProps)` is an ordinary
    // method of this name, and reading `$component` as a reflection filter failed the call, so
    // every page rendering a Twig component died.
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let filter = eval_reflection_member_filter(evaluated_args, values)?;
    if eval_reflection_is_builtin_closure(&reflected_name, context) {
        let names = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            eval_reflection_builtin_closure_method_names()
        } else {
            Vec::new()
        };
        return eval_reflection_member_object_array_result(
            owner_kind,
            "Closure",
            &names,
            filter,
            context,
            values,
        )
        .map(Some);
    }
    if let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) {
        let is_eval_class = context.class(&reflected_name).is_some();
        let names = if owner_kind != EVAL_REFLECTION_OWNER_METHOD {
            metadata.property_names
        } else if is_eval_class {
            context.class_reflection_method_names(&reflected_name)
        } else {
            metadata.method_names
        };
        let result = eval_reflection_member_object_array_result(
            owner_kind,
            &reflected_name,
            &names,
            filter,
            context,
            values,
        );
        let result = if owner_kind == EVAL_REFLECTION_OWNER_METHOD && is_eval_class {
            result.and_then(|result| {
                eval_reflection_append_native_ancestor_methods(
                    &reflected_name,
                    &names,
                    filter,
                    result,
                    context,
                    values,
                )
            })
        } else {
            result
        };
        return result
        .and_then(|result| {
            eval_reflection_object_dynamic_property_array_result(
                object,
                owner_kind,
                &reflected_name,
                filter,
                result,
                context,
                values,
            )
        })
        .map(Some);
    }
    let native_interface_property_names =
        if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
            eval_reflection_native_interface_property_names(&reflected_name, context)
        } else {
            Vec::new()
        };
    let names = if native_interface_property_names.is_empty() {
        eval_reflection_aot_member_names(owner_kind, &reflected_name, values)?
    } else {
        native_interface_property_names
    };
    eval_reflection_aot_member_object_array_result(
        owner_kind,
        &reflected_name,
        &names,
        filter,
        context,
        values,
    )
    .and_then(|result| {
        eval_reflection_object_dynamic_property_array_result(
            object,
            owner_kind,
            &reflected_name,
            filter,
            result,
            context,
            values,
        )
    })
    .map(Some)
}

/// Handles eval-backed `ReflectionClass::getMethod()` and `getProperty()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_member_result(
    object: RuntimeCellHandle,
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let owner_kind = if method_name.eq_ignore_ascii_case("getMethod") {
        EVAL_REFLECTION_OWNER_METHOD
    } else if method_name.eq_ignore_ascii_case("getProperty") {
        EVAL_REFLECTION_OWNER_PROPERTY
    } else {
        return Ok(None);
    };
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let requested_name = eval_reflection_string_arg(args[0], values)?;
    let Some(member_name) =
        eval_reflection_member_name(owner_kind, &reflected_name, &requested_name, context)
    else {
        if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            // Walks the compiled ancestry, as `hasMethod()` does. An eval class's metadata lists
            // only its own methods, so one inherited from a compiled parent was missing:
            // Symfony's `CheckTypeDeclarationsPass` asked `getMethod('setName')` of the
            // interpreted `AboutCommand` right after `hasMethod` said true.
            if let Some(method) = eval_reflection_method_object_result_if_exists(
                &reflected_name,
                &requested_name,
                context,
                values,
            )? {
                return Ok(Some(method));
            }
        }
        if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY
            && !eval_reflection_class_like_exists(&reflected_name, context)
        {
            if let Some((declaring_class, property)) =
                eval_reflection_native_interface_property_requirement(
                    &reflected_name,
                    &requested_name,
                    context,
                )
            {
                let member = eval_reflection_interface_property_metadata(declaring_class, &property);
                return eval_reflection_member_object_result(
                    EVAL_REFLECTION_OWNER_PROPERTY,
                    &requested_name,
                    &member,
                    context,
                    values,
                )
                .map(Some);
            }
            if let Some(member) = eval_reflection_aot_property_metadata_if_exists(
                &reflected_name,
                &requested_name,
                context,
                values,
            )? {
                return eval_reflection_member_object_result(
                    EVAL_REFLECTION_OWNER_PROPERTY,
                    &requested_name,
                    &member,
                    context,
                    values,
                )
                .map(Some);
            }
        }
        if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
            if let Some(dynamic_object) =
                eval_reflection_object_reflected_object(object, context, values)?
            {
                let exists = eval_reflection_object_dynamic_property_exists(
                    dynamic_object,
                    &requested_name,
                    context,
                    values,
                );
                values.release(dynamic_object)?;
                if exists? {
                    let member = eval_reflection_dynamic_property_metadata(&reflected_name);
                    return eval_reflection_member_object_result(
                        EVAL_REFLECTION_OWNER_PROPERTY,
                        &requested_name,
                        &member,
                        context,
                        values,
                    )
                    .map(Some);
                }
            }
        }
        let message_name = eval_reflection_class_like_attributes(&reflected_name, context)
            .map(|metadata| metadata.resolved_name)
            .unwrap_or_else(|| reflected_name.clone());
        let message =
            eval_reflection_missing_member_message(owner_kind, &message_name, &requested_name);
        return eval_throw_reflection_exception(&message, context, values);
    };
    let member =
        eval_reflection_member_metadata(owner_kind, &reflected_name, &member_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_member_object_result(owner_kind, &member_name, &member, context, values)
        .map(Some)
}

/// Returns generated/AOT constant names visible through eval ReflectionClass.
pub(super) fn eval_reflection_aot_constant_names(
    reflected_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let runtime_class_name = reflected_name.trim_start_matches('\\');
    let names_array = values.reflection_constant_names(runtime_class_name)?;
    let names = eval_reflection_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Returns constant names from eval metadata or generated/AOT runtime metadata.
pub(super) fn eval_reflection_constant_names(
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    if context.has_interface(reflected_name) {
        Ok(context.interface_constant_names(reflected_name))
    } else if context.has_trait(reflected_name) {
        Ok(context.trait_constant_names(reflected_name))
    } else if context.has_class(reflected_name) || context.has_enum(reflected_name) {
        Ok(context.class_constant_names(reflected_name))
    } else {
        eval_reflection_aot_constant_names(reflected_name, values)
    }
}

/// Returns a materialized eval constant value for Reflection without visibility checks.
pub(super) fn eval_reflection_eval_constant_value(
    reflected_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(case) = context.enum_case(reflected_name, constant_name) {
        return Ok(Some(case));
    }
    let Some((declaring_class, constant)) = context.class_constant(reflected_name, constant_name)
    else {
        return Ok(None);
    };
    eval_class_like_constant_cell(&declaring_class, &constant, context, values).map(Some)
}

/// Returns a materialized eval or AOT constant value for Reflection without visibility checks.
pub(super) fn eval_reflection_constant_value(
    reflected_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if eval_reflection_class_like_exists(reflected_name, context) {
        return eval_reflection_eval_constant_value(reflected_name, constant_name, context, values);
    }
    let runtime_class_name = reflected_name.trim_start_matches('\\');
    values.reflection_constant_value(runtime_class_name, constant_name)
}

/// Builds one eval-backed `ReflectionClassConstant` object for a visible constant name.
pub(super) fn eval_reflection_class_constant_object_result(
    reflected_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (declaring_class_name, attributes, visibility, is_final, is_enum_case) =
        eval_reflection_class_constant_metadata(reflected_name, constant_name, context, values)?
            .ok_or(EvalStatus::RuntimeFatal)?;
    let constant_value =
        eval_reflection_constant_value(reflected_name, constant_name, context, values)?
            .ok_or(EvalStatus::RuntimeFatal)?;
    let mut flags = eval_reflection_member_flags(visibility, false, is_final, false, false);
    if is_enum_case {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE;
    }
    let modifiers = eval_reflection_class_constant_modifiers(visibility, is_final);
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT,
        constant_name,
        &attributes,
        &[],
        &[],
        &[],
        &[],
        Some(&declaring_class_name),
        &[],
        None,
        None,
        None,
        None,
        flags,
        modifiers,
        0,
        Some(constant_value),
        None,
        context,
        values,
    )
}

/// Returns whether one class constant passes an optional `ReflectionClassConstant` filter.
pub(super) fn eval_reflection_constant_matches_filter(
    reflected_name: &str,
    constant_name: &str,
    filter: Option<u64>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(filter) = filter else {
        return Ok(true);
    };
    Ok(eval_reflection_class_constant_metadata(reflected_name, constant_name, context, values)?
        .is_some_and(|(_, _, visibility, is_final, _)| {
            eval_reflection_class_constant_modifiers(visibility, is_final) & filter != 0
        }))
}

/// Resolves the declared member spelling for eval `ReflectionClass` single-member lookups.
pub(super) fn eval_reflection_member_name(
    owner_kind: u64,
    reflected_name: &str,
    requested_name: &str,
    context: &ElephcEvalContext,
) -> Option<String> {
    if owner_kind == EVAL_REFLECTION_OWNER_METHOD
        && eval_reflection_is_builtin_closure(reflected_name, context)
    {
        return eval_reflection_builtin_closure_method_metadata(requested_name).map(|(name, _)| name);
    }
    let metadata = eval_reflection_class_like_attributes(reflected_name, context)?;
    let names = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        metadata.method_names
    } else {
        metadata.property_names
    };
    names.into_iter().find(|name| match owner_kind {
        EVAL_REFLECTION_OWNER_METHOD => name.eq_ignore_ascii_case(requested_name),
        EVAL_REFLECTION_OWNER_PROPERTY => name == requested_name,
        _ => false,
    })
}

/// Builds PHP-compatible missing-member messages for eval ReflectionClass lookups.
pub(super) fn eval_reflection_missing_member_message(
    owner_kind: u64,
    reflected_name: &str,
    requested_name: &str,
) -> String {
    if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        format!(
            "Method {}::{}() does not exist",
            reflected_name, requested_name
        )
    } else {
        format!(
            "Property {}::${} does not exist",
            reflected_name, requested_name
        )
    }
}

/// Appends the methods an eval class inherits from its compiled ancestor to a `getMethods()` list.
///
/// An eval class that extends a compiled class inherits that class's methods, but the eval
/// metadata only knows the eval part of the chain. PHP lists those inherited methods after the
/// class's own, minus the ones redeclared below and minus privates, which are never inherited.
fn eval_reflection_append_native_ancestor_methods(
    reflected_name: &str,
    listed: &[String],
    filter: Option<u64>,
    mut result: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(native_parent) = context.class_native_parent_name(reflected_name) else {
        return Ok(result);
    };
    let mut seen: std::collections::HashSet<String> = listed.iter().map(|name| name.to_ascii_lowercase()).collect();
    let names = eval_reflection_aot_member_names(EVAL_REFLECTION_OWNER_METHOD, &native_parent, values)?;
    let mut index = values.array_len(result)? as i64;
    for name in &names {
        if !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let Some(member) = eval_reflection_aot_method_metadata_with_signature_if_exists(
            &native_parent,
            name,
            context,
            values,
        )?
        else {
            continue;
        };
        if member.visibility == EvalVisibility::Private
            || !eval_reflection_member_matches_filter(&member, filter)
        {
            continue;
        }
        let member_object = eval_reflection_member_object_result(
            EVAL_REFLECTION_OWNER_METHOD,
            name,
            &member,
            context,
            values,
        )?;
        let key = values.int(index)?;
        result = eval_reflection_array_set_owned(result, key, member_object, values)?;
        index += 1;
    }
    Ok(result)
}
