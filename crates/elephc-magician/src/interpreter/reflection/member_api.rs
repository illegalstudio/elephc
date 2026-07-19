//! Purpose:
//! Implements Reflection APIs for properties, constants, and enum cases.
//!
//! Called from:
//! - `crate::interpreter::statements` for non-callable reflected members.
//!
//! Key details:
//! - Property access, hooks, laziness, raw values, and owner-specific formatting dispatch here.

use super::*;

/// Handles eval-backed `ReflectionProperty` hook-inspection calls.
pub(in crate::interpreter) fn eval_reflection_property_hooks_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    let Some((property_class, property)) =
        eval_reflection_property_for_hooks(&declaring_class, &property_name, context)
    else {
        return Ok(None);
    };
    match method_name.to_ascii_lowercase().as_str() {
        "hashooks" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let has_hooks = !eval_reflection_property_hook_kinds(&property).is_empty();
            values.bool_value(has_hooks).map(Some)
        }
        "hashook" => {
            let hook = eval_reflection_property_hook_arg(evaluated_args, context, values)?;
            values
                .bool_value(eval_reflection_property_has_hook(&property, hook))
                .map(Some)
        }
        "gethook" => {
            let hook = eval_reflection_property_hook_arg(evaluated_args, context, values)?;
            if !eval_reflection_property_has_hook(&property, hook) {
                return values.null().map(Some);
            }
            eval_reflection_property_hook_method_object(
                &property_class,
                &property,
                hook,
                context,
                values,
            )
            .map(Some)
        }
        "gethooks" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_property_hook_method_array(&property_class, &property, context, values)
                .map(Some)
        }
        _ => Ok(None),
    }
}

/// Handles eval-backed `ReflectionProperty::getValue()` calls.
pub(in crate::interpreter) fn eval_reflection_property_get_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getValue") {
        return Ok(None);
    }
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    let object = eval_reflection_property_get_value_arg(evaluated_args)?;
    if context.eval_reflection_property_is_dynamic(identity) {
        let object = object.ok_or(EvalStatus::RuntimeFatal)?;
        return eval_reflection_dynamic_property_get_value(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
        .map(Some);
    }
    let Some(member) =
        eval_reflection_reflected_property_metadata(&declaring_class, &property_name, context, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if member.is_static {
        return eval_reflection_static_property_value(
            &declaring_class,
            &property_name,
            context,
            values,
        )?
        .map(Some)
        .ok_or(EvalStatus::RuntimeFatal);
    }
    let object = object.ok_or(EvalStatus::RuntimeFatal)?;
    if eval_reflection_class_like_exists(&declaring_class, context) {
        eval_reflection_instance_property_get_value(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    } else {
        eval_reflection_aot_instance_property_get_value(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    }
    .map(Some)
}

/// Handles eval-backed `ReflectionProperty::setValue()` calls.
pub(in crate::interpreter) fn eval_reflection_property_set_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("setValue") {
        return Ok(None);
    }
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    let (object_or_value, value) = eval_reflection_property_set_value_args(evaluated_args)?;
    if context.eval_reflection_property_is_dynamic(identity) {
        let value = value.ok_or(EvalStatus::RuntimeFatal)?;
        eval_reflection_dynamic_property_set_value(
            &declaring_class,
            &property_name,
            object_or_value,
            value,
            context,
            values,
        )?;
        return values.null().map(Some);
    }
    let Some(member) =
        eval_reflection_reflected_property_metadata(&declaring_class, &property_name, context, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if member.is_static {
        let value = value.unwrap_or(object_or_value);
        if eval_reflection_class_like_exists(&declaring_class, context) {
            let declaring_class = member
                .declaring_class_name
                .as_deref()
                .ok_or(EvalStatus::RuntimeFatal)?;
            if let Some(replaced) =
                context.set_static_property(declaring_class, &property_name, value)
            {
                values.release(replaced)?;
            }
        } else {
            let declaring_class = member
                .declaring_class_name
                .as_deref()
                .unwrap_or(declaring_class.as_str());
            let updated = eval_reflection_with_declaring_class_scope(declaring_class, context, |_| {
                values.static_property_set(declaring_class, &property_name, value)
            })?;
            if !updated {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        return values.null().map(Some);
    }
    let value = value.ok_or(EvalStatus::RuntimeFatal)?;
    if eval_reflection_class_like_exists(&declaring_class, context) {
        eval_reflection_instance_property_set_value(
            &declaring_class,
            &property_name,
            object_or_value,
            value,
            context,
            values,
        )?;
    } else {
        eval_reflection_aot_instance_property_set_value(
            &declaring_class,
            &property_name,
            object_or_value,
            value,
            context,
            values,
        )?;
    }
    values.null().map(Some)
}

/// Handles `ReflectionProperty::isInitialized()` calls for eval and generated/AOT properties.
pub(in crate::interpreter) fn eval_reflection_property_is_initialized_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("isInitialized") {
        return Ok(None);
    }
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    let object = eval_reflection_property_get_value_arg(evaluated_args)?;
    if context.eval_reflection_property_is_dynamic(identity) {
        let object = object.ok_or(EvalStatus::RuntimeFatal)?;
        return eval_reflection_dynamic_property_is_initialized(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
        .and_then(|initialized| values.bool_value(initialized))
        .map(Some);
    }
    let Some(member) =
        eval_reflection_reflected_property_metadata(&declaring_class, &property_name, context, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if member.is_static {
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .ok_or(EvalStatus::RuntimeFatal)?;
        let initialized = if eval_reflection_class_like_exists(declaring_class, context) {
            context
                .static_property(declaring_class, &property_name)
                .is_some()
        } else {
            eval_reflection_aot_static_property_is_initialized(
                declaring_class,
                &property_name,
                context,
                values,
            )?
        };
        return values.bool_value(initialized).map(Some);
    }
    let object = object.ok_or(EvalStatus::RuntimeFatal)?;
    if eval_reflection_class_like_exists(&declaring_class, context) {
        eval_reflection_instance_property_is_initialized(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    } else {
        eval_reflection_aot_instance_property_is_initialized(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    }
    .and_then(|initialized| values.bool_value(initialized))
    .map(Some)
}

/// Handles `ReflectionProperty::isLazy()` and `skipLazyInitialization()` calls.
pub(in crate::interpreter) fn eval_reflection_property_lazy_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    if method_name.eq_ignore_ascii_case("isLazy") {
        let object = eval_reflection_property_raw_value_arg(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            eval_reflection_dynamic_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
            return values.bool_value(false).map(Some);
        }
        if eval_reflection_class_like_exists(&declaring_class, context) {
            eval_reflection_property_validate_object(&declaring_class, object, context, values)?;
        } else {
            eval_reflection_aot_instance_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
        }
        return values.bool_value(false).map(Some);
    }
    if method_name.eq_ignore_ascii_case("skipLazyInitialization") {
        let object = eval_reflection_property_raw_value_arg(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            eval_reflection_dynamic_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
            return Err(EvalStatus::RuntimeFatal);
        }
        if eval_reflection_class_like_exists(&declaring_class, context) {
            let (_, property) = eval_reflection_instance_property_target(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )?;
            if property.is_virtual() {
                return Err(EvalStatus::RuntimeFatal);
            }
        } else {
            eval_reflection_aot_instance_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
        }
        return values.null().map(Some);
    }
    Ok(None)
}

/// Handles eval-backed `ReflectionProperty::__toString()` calls.
pub(in crate::interpreter) fn eval_reflection_property_to_string_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
        return Ok(None);
    }
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    if context.eval_reflection_property_is_dynamic(identity) {
        let member = eval_reflection_dynamic_property_metadata(&declaring_class);
        let text = eval_reflection_property_to_string(&property_name, &member);
        return values.string(&text).map(Some);
    }
    let member =
        eval_reflection_reflected_property_metadata(&declaring_class, &property_name, context, values)?
            .ok_or(EvalStatus::RuntimeFatal)?;
    let text = eval_reflection_property_to_string(&property_name, &member);
    values.string(&text).map(Some)
}

/// Handles eval-backed `ReflectionClassConstant` and enum-case `__toString()` calls.
pub(in crate::interpreter) fn eval_reflection_class_constant_to_string_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
        return Ok(None);
    }
    let Some((declaring_class, constant_name, owner_kind)) =
        context
            .eval_reflection_class_constant(identity)
            .map(|(declaring_class, constant_name, owner_kind)| {
                (
                    declaring_class.to_string(),
                    constant_name.to_string(),
                    owner_kind,
                )
            })
    else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    let text = eval_reflection_class_constant_to_string(
        &declaring_class,
        &constant_name,
        owner_kind,
        context,
        values,
    )?;
    values.string(&text).map(Some)
}

/// Handles `ReflectionEnumUnitCase::getEnum()` and `ReflectionEnumBackedCase::getEnum()`.
pub(in crate::interpreter) fn eval_reflection_enum_case_get_enum_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getEnum") {
        return Ok(None);
    }
    eval_reflection_bind_no_args(evaluated_args)?;
    let Some((declaring_class, _, owner_kind)) = context
        .eval_reflection_class_constant(identity)
        .map(|(class, constant, owner_kind)| (class.to_string(), constant.to_string(), owner_kind))
    else {
        return Ok(None);
    };
    if !matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
    ) {
        return Ok(None);
    }
    eval_reflection_enum_object_result(&declaring_class, context, values).map(Some)
}

/// Handles `ReflectionProperty::getRawValue()` and raw write calls.
pub(in crate::interpreter) fn eval_reflection_property_raw_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    if method_name.eq_ignore_ascii_case("getRawValue") {
        let object = eval_reflection_property_raw_value_arg(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            return eval_reflection_dynamic_property_get_value(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )
            .map(Some);
        }
        return if eval_reflection_class_like_exists(&declaring_class, context) {
            eval_reflection_instance_property_get_raw_value(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )
        } else {
            eval_reflection_aot_instance_property_get_value(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )
        }
        .map(Some);
    }
    if method_name.eq_ignore_ascii_case("setRawValue")
        || method_name.eq_ignore_ascii_case("setRawValueWithoutLazyInitialization")
    {
        let (object, value) = eval_reflection_property_set_raw_value_args(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            eval_reflection_dynamic_property_set_value(
                &declaring_class,
                &property_name,
                object,
                value,
                context,
                values,
            )?;
            return values.null().map(Some);
        }
        if eval_reflection_class_like_exists(&declaring_class, context) {
            eval_reflection_instance_property_set_raw_value(
                &declaring_class,
                &property_name,
                object,
                value,
                context,
                values,
            )?;
        } else {
            eval_reflection_aot_instance_property_set_value(
                &declaring_class,
                &property_name,
                object,
                value,
                context,
                values,
            )?;
        }
        return values.null().map(Some);
    }
    Ok(None)
}
