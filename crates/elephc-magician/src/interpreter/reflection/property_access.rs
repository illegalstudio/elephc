//! Purpose:
//! Reads, writes, and validates reflected instance, static, and dynamic properties.
//!
//! Called from:
//! - ReflectionProperty value, raw-value, and initialized-state APIs.
//!
//! Key details:
//! - Eval and AOT storage paths enforce declaring-class and object compatibility.

use super::*;

/// Reads one eval instance property through ReflectionProperty semantics.
pub(super) fn eval_reflection_instance_property_get_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (object_class_name, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.has_get_hook()
        && !current_eval_property_hook_is(
            declaring_class,
            property.name(),
            &property_hook_get_method(property.name()),
            context,
        )
    {
        let (hook_class, hook_method) = context
            .class_method(
                &object_class_name,
                &property_hook_get_method(property.name()),
            )
            .ok_or(EvalStatus::RuntimeFatal)?;
        return eval_dynamic_method_with_values(
            &hook_class,
            &object_class_name,
            &hook_method,
            object,
            Vec::new(),
            context,
            values,
        );
    }
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_get(object, &storage_property_name)
}

/// Writes one eval instance property through ReflectionProperty semantics.
pub(super) fn eval_reflection_instance_property_set_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (object_class_name, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    validate_eval_reflection_property_write(declaring_class, &property, context)?;
    if property.has_set_hook() {
        if !current_eval_property_hook_is(
            declaring_class,
            property.name(),
            &property_hook_set_method(property.name()),
            context,
        ) {
            let (hook_class, hook_method) = context
                .class_method(
                    &object_class_name,
                    &property_hook_set_method(property.name()),
                )
                .ok_or(EvalStatus::RuntimeFatal)?;
            let hook_result = eval_dynamic_method_with_values(
                &hook_class,
                &object_class_name,
                &hook_method,
                object,
                vec![EvaluatedCallArg {
                    name: None,
                    value,
                    ref_target: None,
                }],
                context,
                values,
            )?;
            values.release(hook_result)?;
            return Ok(());
        }
    } else if property.has_get_hook() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_set(object, &storage_property_name, value)?;
    let identity = values.object_identity(object)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Reads one generated/AOT instance property through ReflectionProperty semantics.
pub(super) fn eval_reflection_aot_instance_property_get_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_aot_instance_property_validate_object(
        declaring_class,
        object,
        context,
        values,
    )?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, |_| {
        values.property_get(object, property_name)
    })
}

/// Writes one generated/AOT instance property through ReflectionProperty semantics.
pub(super) fn eval_reflection_aot_instance_property_set_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_reflection_aot_instance_property_validate_object(
        declaring_class,
        object,
        context,
        values,
    )?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, |_| {
        values.property_set(object, property_name, value)
    })
}

/// Checks one generated/AOT instance property initialization marker through ReflectionProperty.
pub(super) fn eval_reflection_aot_instance_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_reflection_aot_instance_property_validate_object(
        declaring_class,
        object,
        context,
        values,
    )?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, |_| {
        values.property_is_initialized(object, property_name)
    })
}

/// Checks one generated/AOT static property initialization marker through ReflectionProperty.
pub(super) fn eval_reflection_aot_static_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_reflection_with_declaring_class_scope(declaring_class, context, |_| {
        values.static_property_is_initialized(declaring_class, property_name)
    })
}

/// Verifies a generated/AOT ReflectionProperty instance target is compatible.
pub(super) fn eval_reflection_aot_instance_property_validate_object(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let is_instance = dynamic_object_is_a(object, declaring_class, false, context, values)?
        .map_or_else(|| values.object_is_a(object, declaring_class, false), Ok)?;
    if is_instance {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether one eval instance property is initialized for ReflectionProperty.
pub(super) fn eval_reflection_instance_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let (_, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.is_virtual() {
        return Ok(true);
    }
    let identity = values.object_identity(object)?;
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    Ok(context.dynamic_property_is_initialized(identity, &storage_property_name))
}

/// Reads one eval instance property through ReflectionProperty raw-storage semantics.
pub(super) fn eval_reflection_instance_property_get_raw_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (_, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.is_virtual() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_get(object, &storage_property_name)
}

/// Writes one eval instance property through ReflectionProperty raw-storage semantics.
pub(super) fn eval_reflection_instance_property_set_raw_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (_, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.is_virtual() {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_reflection_property_write(declaring_class, &property, context)?;
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_set(object, &storage_property_name, value)?;
    let identity = values.object_identity(object)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Reads a public dynamic property through ReflectionProperty semantics.
pub(super) fn eval_reflection_dynamic_property_get_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_dynamic_property_validate_object(declaring_class, object, context, values)?;
    values.property_get(object, property_name)
}

/// Writes a public dynamic property through ReflectionProperty semantics.
pub(super) fn eval_reflection_dynamic_property_set_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_reflection_dynamic_property_validate_object(declaring_class, object, context, values)?;
    values.property_set(object, property_name, value)?;
    let identity = values.object_identity(object)?;
    context.mark_dynamic_property_initialized(identity, property_name);
    Ok(())
}

/// Returns whether a public dynamic property currently exists on the target object.
pub(super) fn eval_reflection_dynamic_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_reflection_dynamic_property_validate_object(declaring_class, object, context, values)?;
    eval_reflection_object_dynamic_property_exists(object, property_name, values)
}

/// Validates the object argument used by dynamic ReflectionProperty operations.
pub(super) fn eval_reflection_dynamic_property_validate_object(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let object_class_name = eval_reflection_object_class_name(object, context, values)?;
    if eval_reflection_class_like_exists(declaring_class, context) {
        if context.class_is_a(&object_class_name, declaring_class, false) {
            return Ok(());
        }
    } else if object_class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(declaring_class.trim_start_matches('\\'))
    {
        return Ok(());
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Validates the object argument shared by non-mutating ReflectionProperty instance APIs.
pub(super) fn eval_reflection_property_validate_object(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let object_class_name = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
        .ok_or(EvalStatus::RuntimeFatal)?;
    if !context.class_is_a(&object_class_name, declaring_class, false) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Resolves and validates the object/property pair targeted by ReflectionProperty.
pub(super) fn eval_reflection_instance_property_target(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(String, EvalClassProperty), EvalStatus> {
    let identity = values.object_identity(object)?;
    let object_class_name = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
        .ok_or(EvalStatus::RuntimeFatal)?;
    if !context.class_is_a(&object_class_name, declaring_class, false) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (_, property) = context
        .class_own_property(declaring_class, property_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    if property.is_static() || property.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok((object_class_name, property))
}

/// Rejects writes to eval properties ReflectionProperty is not allowed to mutate.
pub(super) fn validate_eval_reflection_property_write(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if !property.is_readonly() {
        return Ok(());
    }
    current_eval_property_hook_is(
        declaring_class,
        property.name(),
        &property_hook_set_method(property.name()),
        context,
    )
    .then_some(())
    .ok_or(EvalStatus::RuntimeFatal)
}

/// Throws PHP's `ReflectionException` for invalid static-property writes.
pub(super) fn eval_reflection_static_property_missing_for_set(
    reflected_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_throw_reflection_exception(
        &format!(
            "Class {} does not have a property named {}",
            reflected_name, property_name
        ),
        context,
        values,
    )
}

/// Returns ReflectionProperty default metadata for concrete eval properties.
pub(super) fn eval_reflection_property_default_value(property: &EvalClassProperty) -> Option<EvalExpr> {
    if let Some(default) = property.default() {
        return Some(default.clone());
    }
    if property.is_abstract() || property.property_type().is_some() {
        return None;
    }
    Some(EvalExpr::Const(EvalConst::Null))
}
