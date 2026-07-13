//! Purpose:
//! Reads, writes, binds, tests, and unsets eval instance properties.
//!
//! Called from:
//! - Expression and statement dispatch for object property operations.
//!
//! Key details:
//! - Declared, dynamic, magic, hooked, and reference-backed properties share visibility checks.

use super::*;

/// Reads one object property while enforcing eval-declared member visibility.
pub(in crate::interpreter) fn eval_property_get_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_get(object, property_name);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_runtime_object_class_name(object, values)?;
        if let Some((declaring_class, visibility, _, is_static)) =
            eval_reflection_aot_property_access_metadata(&class_name, property_name, values)?
        {
            if !is_static && validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_throw_property_access_error(
                    &declaring_class,
                    property_name,
                    visibility,
                    context,
                    values,
                );
            }
        }
        return values.property_get(object, property_name);
    };
    let object_class_name = class.name().to_string();
    let mut storage_property_name = property_name.to_string();
    let mut declared_property_found = false;
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        declared_property_found = true;
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            if let Some(result) =
                eval_magic_property_get(object, &object_class_name, property_name, context, values)?
            {
                return Ok(result);
            }
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
        if property.has_get_hook()
            && !current_eval_property_hook_is(
                &declaring_class,
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
        if property.property_type().is_some()
            && !context.dynamic_property_is_initialized(identity, &storage_property_name)
        {
            return eval_throw_uninitialized_property_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
    }
    if !declared_property_found
        && eval_object_public_property_exists(object, property_name, values)?
    {
        return values.property_get(object, property_name);
    }
    if !declared_property_found {
        if let Some((declaring_class, visibility, _, is_static)) =
            eval_dynamic_class_native_property_metadata(
                &object_class_name,
                property_name,
                context,
                values,
            )?
        {
            if !is_static {
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    if let Some(result) = eval_magic_property_get(
                        object,
                        &object_class_name,
                        property_name,
                        context,
                        values,
                    )? {
                        return Ok(result);
                    }
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        visibility,
                        context,
                        values,
                    );
                }
                return eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.property_get(object, property_name)
                });
            }
        }
    }
    if !declared_property_found {
        if let Some(result) =
            eval_magic_property_get(object, &object_class_name, property_name, context, values)?
        {
            return Ok(result);
        }
    }
    if let Some(target) = context
        .dynamic_property_alias(identity, &storage_property_name)
        .cloned()
    {
        return eval_reference_target_value(&target, context, values);
    }
    values.property_get(object, &storage_property_name)
}

/// Writes one object property while enforcing eval-declared member visibility.
pub(in crate::interpreter) fn eval_property_set_result(
    object: RuntimeCellHandle,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_set(object, property_name, value);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_runtime_object_class_name(object, values)?;
        if let Some((declaring_class, _, write_visibility, is_static)) =
            eval_reflection_aot_property_access_metadata(&class_name, property_name, values)?
        {
            if !is_static
                && validate_eval_member_access(&declaring_class, write_visibility, context).is_err()
            {
                return eval_throw_property_access_error(
                    &declaring_class,
                    property_name,
                    write_visibility,
                    context,
                    values,
                );
            }
        }
        return values.property_set(object, property_name, value);
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class_is_readonly = class.is_readonly_class();
    let mut storage_property_name = property_name.to_string();
    let mut declared_property_found = false;
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        declared_property_found = true;
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            if eval_magic_property_set(
                object,
                &object_class_name,
                property_name,
                value,
                context,
                values,
            )? {
                return Ok(());
            }
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
            return eval_throw_property_write_access_error(
                &declaring_class,
                &property,
                context,
                values,
            );
        }
        if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
        if property.has_set_hook() {
            if !current_eval_property_hook_is(
                &declaring_class,
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
            return eval_throw_property_hook_readonly_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
    }
    if !declared_property_found {
        if let Some((declaring_class, _, write_visibility, is_static)) =
            eval_dynamic_class_native_property_metadata(
                &object_class_name,
                property_name,
                context,
                values,
            )?
        {
            if !is_static {
                if validate_eval_member_access(&declaring_class, write_visibility, context)
                    .is_err()
                {
                    if eval_magic_property_set(
                        object,
                        &object_class_name,
                        property_name,
                        value,
                        context,
                        values,
                    )? {
                        return Ok(());
                    }
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        write_visibility,
                        context,
                        values,
                    );
                }
                return eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.property_set(object, property_name, value)
                });
            }
        }
    }
    if !declared_property_found
        && eval_magic_property_set(
            object,
            &object_class_name,
            property_name,
            value,
            context,
            values,
        )?
    {
        return Ok(());
    }
    if !declared_property_found && class_is_readonly {
        return eval_throw_dynamic_property_creation_error(
            &object_class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some(target) = context
        .dynamic_property_alias(identity, &storage_property_name)
        .cloned()
    {
        eval_reference_target_write(
            identity,
            &storage_property_name,
            target,
            value,
            context,
            values,
        )?;
        context.mark_dynamic_property_initialized(identity, &storage_property_name);
        return values.property_set(object, &storage_property_name, value);
    }
    values.property_set(object, &storage_property_name, value)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Binds one eval object property to a by-reference source parameter.
pub(super) fn eval_property_reference_bind_result(
    object: RuntimeCellHandle,
    property_name: &str,
    source_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let class = context
        .dynamic_object_class(identity)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (declaring_class, property) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    validate_eval_property_write_access(&declaring_class, &property, context)?;
    if property.is_readonly() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let storage_property_name = eval_instance_property_storage_name(&declaring_class, &property);
    let target = eval_property_reference_target(
        identity,
        &storage_property_name,
        source_name,
        context,
        scope,
        values,
    )?;
    let value = eval_reference_target_value(&target, context, values)?;
    context.bind_dynamic_property_alias(identity, &storage_property_name, target);
    values.property_set(object, &storage_property_name, value)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Resolves a local by-reference source into a persistent property alias target.
pub(super) fn eval_property_reference_target(
    object_identity: u64,
    storage_property_name: &str,
    source_name: &str,
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReferenceTarget, EvalStatus> {
    if let Some(target) = scope.reference_target(source_name).cloned() {
        return Ok(target);
    }
    if context.current_function().is_some() {
        let cell =
            visible_scope_cell(context, scope, source_name).map_or_else(|| values.null(), Ok)?;
        return Ok(EvalReferenceTarget::Cell { cell });
    }
    let alias_name = eval_property_reference_alias_name(object_identity, storage_property_name);
    for replaced in set_reference_alias(context, scope, &alias_name, source_name, values)? {
        values.release(replaced)?;
    }
    Ok(EvalReferenceTarget::Variable {
        scope: scope as *mut ElephcEvalScope,
        name: alias_name,
    })
}

/// Builds the hidden scope variable name that backs one property reference alias.
pub(super) fn eval_property_reference_alias_name(object_identity: u64, storage_property_name: &str) -> String {
    format!("\0elephc_property_ref:{object_identity}:{storage_property_name}")
}

/// Reads the current value from a persistent reference target.
pub(in crate::interpreter) fn eval_reference_target_value(
    target: &EvalReferenceTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalReferenceTarget::ArrayElement {
            scope,
            array_name,
            index,
        } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let array =
                visible_scope_cell(context, scope, array_name).map_or_else(|| values.null(), Ok)?;
            values.array_get(array, *index)
        }
        EvalReferenceTarget::NestedArrayElement {
            array_target,
            index,
        } => {
            let array = eval_reference_target_value(array_target, context, values)?;
            values.array_get(array, *index)
        }
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => {
            let previous_scope = context.replace_execution_scope(access_scope.clone());
            let result = eval_property_get_result(*object, property, context, values);
            context.replace_execution_scope(previous_scope);
            result
        }
        EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        } => {
            let previous_scope = context.replace_execution_scope(access_scope.clone());
            let result = eval_static_property_get_result(class_name, property, context, values);
            context.replace_execution_scope(previous_scope);
            result
        }
        EvalReferenceTarget::Cell { cell } => Ok(*cell),
        EvalReferenceTarget::InvokerSlot { slot, source_tag } => {
            eval_invoker_slot_ref_target_value(*slot, *source_tag, values)
        }
    }
}

/// Writes a new value to a persistent reference target.
pub(super) fn eval_reference_target_write(
    object_identity: u64,
    storage_property_name: &str,
    target: EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if matches!(target, EvalReferenceTarget::Cell { .. }) {
        context.bind_dynamic_property_alias(
            object_identity,
            storage_property_name,
            EvalReferenceTarget::Cell { cell: value },
        );
        return Ok(());
    }
    write_back_method_ref_target(&target, value, context, values)
}

/// Evaluates PHP `isset($object->property)` without forcing `__get()` first.
pub(in crate::interpreter) fn eval_property_isset_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    };
    let object_class_name = class.name().to_string();
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_ok() {
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            if property.property_type().is_some()
                && !context.dynamic_property_is_initialized(identity, &storage_property_name)
            {
                return Ok(false);
            }
            let value = eval_property_get_result(object, property_name, context, values)?;
            return Ok(!values.is_null(value)?);
        }
        return eval_magic_property_isset(
            object,
            &object_class_name,
            property_name,
            context,
            values,
        )
        .map(|result| result.unwrap_or(false));
    }
    if eval_object_public_property_exists(object, property_name, values)? {
        let value = values.property_get(object, property_name)?;
        return Ok(!values.is_null(value)?);
    }
    if let Some((declaring_class, visibility, _, is_static)) =
        eval_dynamic_class_native_property_metadata(
            &object_class_name,
            property_name,
            context,
            values,
        )?
    {
        if !is_static {
            if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_magic_property_isset(
                    object,
                    &object_class_name,
                    property_name,
                    context,
                    values,
                )
                .map(|result| result.unwrap_or(false));
            }
            if !eval_with_native_bridge_scope(&declaring_class, context, || {
                values.property_is_initialized(object, property_name)
            })? {
                return Ok(false);
            }
            let value = eval_with_native_bridge_scope(&declaring_class, context, || {
                values.property_get(object, property_name)
            })?;
            return Ok(!values.is_null(value)?);
        }
    }
    eval_magic_property_isset(object, &object_class_name, property_name, context, values)
        .map(|result| result.unwrap_or(false))
}

/// Evaluates PHP `unset($object->property)` for eval-declared object receivers.
pub(in crate::interpreter) fn eval_property_unset_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return Ok(());
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_ok() {
            if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
                return eval_throw_property_unset_access_error(
                    &declaring_class,
                    &property,
                    context,
                    values,
                );
            }
            if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
                return eval_throw_readonly_property_unset_error(
                    &declaring_class,
                    property.name(),
                    context,
                    values,
                );
            }
            let storage_property_name =
                eval_instance_property_storage_name(&declaring_class, &property);
            context.remove_dynamic_property_alias(identity, &storage_property_name);
            context.mark_dynamic_property_uninitialized(identity, &storage_property_name);
            let null = values.null()?;
            return values.property_set(object, &storage_property_name, null);
        }
        if eval_magic_property_unset(object, &object_class_name, property_name, context, values)? {
            return Ok(());
        }
        return Ok(());
    }
    if eval_object_public_property_exists(object, property_name, values)? {
        let null = values.null()?;
        return values.property_set(object, property_name, null);
    }
    let _ = eval_magic_property_unset(object, &object_class_name, property_name, context, values)?;
    Ok(())
}

/// Dispatches an undefined or inaccessible eval property read through `__get()`.
pub(super) fn eval_magic_property_get(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__get") else {
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    )
    .map(Some)
}

/// Dispatches an undefined or inaccessible eval property write through `__set()`.
pub(super) fn eval_magic_property_set(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__set") else {
        return Ok(false);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property, value]),
        context,
        values,
    )?;
    values.release(result)?;
    Ok(true)
}

/// Dispatches an undefined or inaccessible eval property probe through `__isset()`.
pub(super) fn eval_magic_property_isset(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__isset") else {
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    )?;
    let truthy = values.truthy(result)?;
    values.release(result)?;
    Ok(Some(truthy))
}

/// Dispatches an undefined or inaccessible eval property unset through `__unset()`.
pub(super) fn eval_magic_property_unset(
    object: RuntimeCellHandle,
    object_class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(object_class_name, "__unset") else {
        return Ok(false);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let property = values.string(property_name)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        object_class_name,
        &method,
        object,
        positional_args(vec![property]),
        context,
        values,
    )?;
    values.release(result)?;
    Ok(true)
}

/// Returns whether the object already has a public dynamic property with this exact name.
pub(super) fn eval_object_public_property_exists(
    object: RuntimeCellHandle,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        if key_bytes? == property_name.as_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Validates that an object property may be used as a by-reference method argument.
pub(in crate::interpreter) fn validate_property_ref_target(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return Ok(());
    };
    let object_class_name = class.name().to_string();
    if context.has_enum(&object_class_name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    {
        validate_eval_member_access(&declaring_class, property.visibility(), context)?;
        if property.is_readonly() {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns true while executing the named hook accessor for one property.
pub(in crate::interpreter) fn current_eval_property_hook_is(
    declaring_class: &str,
    property_name: &str,
    hook_method: &str,
    context: &ElephcEvalContext,
) -> bool {
    let Some(current_class) = context.current_class_scope() else {
        return false;
    };
    if !same_eval_class_name(current_class, declaring_class) {
        return false;
    }
    let Some((_, method)) = context
        .current_function()
        .and_then(|function| function.rsplit_once("::"))
    else {
        return false;
    };
    method.eq_ignore_ascii_case(hook_method)
        || method.eq_ignore_ascii_case(&property_hook_get_method(property_name))
        || method.eq_ignore_ascii_case(&property_hook_set_method(property_name))
}
