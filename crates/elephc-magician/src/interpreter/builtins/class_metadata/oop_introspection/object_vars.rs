//! Purpose:
//! Implements `get_object_vars()` for eval-declared and generated/AOT objects.
//!
//! Called from:
//! - `crate::interpreter::builtins::class_metadata::oop_introspection`.
//! - Declarative class metadata builtin dispatch hooks.
//!
//! Key details:
//! - Declared eval properties use storage-name filtering so inaccessible
//!   protected/private slots do not leak as public dynamic properties.

use super::*;
use std::collections::HashSet;

/// Evaluates `get_object_vars()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_get_object_vars(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_get_object_vars_result(&[object], context, values)
}

/// Evaluates materialized `get_object_vars()` arguments.
pub(in crate::interpreter) fn eval_get_object_vars_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if values.type_tag(*object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Ok(identity) = values.object_identity(*object) else {
        return eval_public_object_vars_result(*object, values);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_object_class_metadata_name(*object, context, values)?;
        return eval_runtime_object_vars_result(*object, &class_name, context, values);
    };
    let class_name = class.name().to_string();
    eval_dynamic_object_vars_result(*object, &class_name, context, values)
}

/// Builds `get_object_vars()` for an eval-declared object.
fn eval_dynamic_object_vars_result(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    let mut result = values.assoc_new(property_count)?;
    let mut emitted_keys = HashSet::new();
    let storage_keys = eval_declared_object_storage_names(class_name, context);
    result = eval_add_enum_object_vars(result, object, class_name, &mut emitted_keys, context, values)?;
    result = eval_add_declared_object_vars(
        result,
        object,
        class_name,
        &mut emitted_keys,
        context,
        values,
    )?;
    eval_add_dynamic_object_vars(result, object, &mut emitted_keys, &storage_keys, values)
}

/// Builds `get_object_vars()` for generated/AOT objects from reflection metadata.
fn eval_runtime_object_vars_result(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_names = values.reflection_property_names(class_name)?;
    let declared_names = eval_runtime_string_array_to_vec(property_names, values)?;
    values.release(property_names)?;
    let property_count = values.object_property_len(object)?;
    let mut result = values.assoc_new(declared_names.len() + property_count)?;
    let mut emitted_keys = HashSet::new();
    result = eval_add_runtime_scope_private_object_vars(
        result,
        object,
        &mut emitted_keys,
        context,
        values,
    )?;
    result = eval_add_runtime_declared_object_vars(
        result,
        object,
        class_name,
        &declared_names,
        &mut emitted_keys,
        context,
        values,
    )?;
    eval_add_dynamic_object_vars(result, object, &mut emitted_keys, &HashSet::new(), values)
}

/// Adds generated/AOT private properties declared by the current eval class scope.
fn eval_add_runtime_scope_private_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    emitted_keys: &mut HashSet<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(current_class) = context.current_class_scope() else {
        return Ok(result);
    };
    if !values.object_is_a(object, current_class, false)? {
        return Ok(result);
    }
    let property_names = values.reflection_property_names(current_class)?;
    let declared_names = eval_runtime_string_array_to_vec(property_names, values)?;
    values.release(property_names)?;
    for property_name in declared_names {
        let Some((_, visibility, is_static)) =
            eval_runtime_property_access_metadata(current_class, &property_name, values)?
        else {
            continue;
        };
        if is_static
            || visibility != EvalVisibility::Private
            || emitted_keys.contains(&property_name)
            || !values.property_is_initialized(object, &property_name)?
        {
            continue;
        }
        emitted_keys.insert(property_name.clone());
        let key = values.string(&property_name)?;
        let value = values.property_get(object, &property_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Adds generated/AOT declared instance properties visible from the current eval scope.
fn eval_add_runtime_declared_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    class_name: &str,
    property_names: &[String],
    emitted_keys: &mut HashSet<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    for property_name in property_names {
        let Some((declaring_class, visibility, is_static)) =
            eval_runtime_property_access_metadata(class_name, property_name, values)?
        else {
            continue;
        };
        if is_static
            || validate_eval_member_access(&declaring_class, visibility, context).is_err()
            || emitted_keys.contains(property_name)
            || !values.property_is_initialized(object, property_name)?
        {
            continue;
        }
        emitted_keys.insert(property_name.clone());
        let key = values.string(property_name)?;
        let value = values.property_get(object, property_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Adds synthetic enum properties exposed by PHP enum case objects.
fn eval_add_enum_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    class_name: &str,
    emitted_keys: &mut HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(enum_decl) = context.enum_decl(class_name) else {
        return Ok(result);
    };
    let is_backed = enum_decl.backing_type().is_some();
    result = eval_add_object_var(result, object, "name", emitted_keys, context, values)?;
    if is_backed {
        result = eval_add_object_var(result, object, "value", emitted_keys, context, values)?;
    }
    Ok(result)
}

/// Adds declared instance properties visible from the current eval scope.
fn eval_add_declared_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    class_name: &str,
    emitted_keys: &mut HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let identity = values.object_identity(object)?;
    for class in context.class_chain(class_name) {
        for property in class.properties() {
            if property.is_static()
                || validate_eval_member_access(class.name(), property.visibility(), context)
                    .is_err()
                || emitted_keys.contains(property.name())
            {
                continue;
            }
            let storage_property_name = eval_instance_property_storage_name(class.name(), property);
            if !property.is_virtual()
                && !context.dynamic_property_is_initialized(identity, &storage_property_name)
            {
                continue;
            }
            result = eval_add_object_var(
                result,
                object,
                property.name(),
                emitted_keys,
                context,
                values,
            )?;
        }
    }
    Ok(result)
}

/// Adds one visible object variable to an associative result array.
fn eval_add_object_var(
    result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    property_name: &str,
    emitted_keys: &mut HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    emitted_keys.insert(property_name.to_string());
    let key = values.string(property_name)?;
    let value = eval_property_get_result(object, property_name, context, values)?;
    values.array_set(result, key, value)
}

/// Adds public dynamic properties that are not declared storage slots.
fn eval_add_dynamic_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    emitted_keys: &mut HashSet<String>,
    storage_keys: &HashSet<String>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        let key_name = String::from_utf8(key_bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
        if key_name.contains('\0')
            || storage_keys.contains(&key_name)
            || !emitted_keys.insert(key_name.clone())
        {
            continue;
        }
        let key = values.string(&key_name)?;
        let value = values.property_get(object, &key_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns physical storage names used by declared eval object properties.
fn eval_declared_object_storage_names(
    class_name: &str,
    context: &ElephcEvalContext,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for class in context.class_chain(class_name) {
        for property in class.properties() {
            names.insert(eval_instance_property_storage_name(class.name(), property));
        }
    }
    names
}

/// Builds `get_object_vars()` for runtime objects with public bridge-visible properties.
fn eval_public_object_vars_result(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    let mut result = values.assoc_new(property_count)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        let key_name = String::from_utf8(key_bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.string(&key_name)?;
        let value = values.property_get(object, &key_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns whether an object has a public bridge-visible property by exact name.
pub(in crate::interpreter) fn eval_object_public_property_exists(
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
