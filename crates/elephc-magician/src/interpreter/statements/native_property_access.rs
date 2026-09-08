//! Purpose:
//! Dispatches authorized native property reads and writes through AOT property hooks.
//!
//! Called from:
//! - Instance property access and the non-raw ReflectionProperty value APIs.
//!
//! Key details:
//! - Caller visibility is checked before entry; accessor calls use their declaring scope.
//! - Initializers and raw reflection APIs bypass these PHP-visible accessors.

use super::*;

/// Reads a native property through its getter, or its backing storage when no getter exists.
pub(in crate::interpreter) fn eval_native_property_get_authorized(
    object: RuntimeCellHandle,
    declaring_class: &str,
    property: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_native_property_hook(
        object, declaring_class, property, None, context, values,
    )? {
        return Ok(result);
    }
    if native_property_is_virtual(declaring_class, property, values)? {
        return eval_throw_error(
            &format!("Cannot read write-only property {}::${property}", declaring_class.trim_start_matches('\\')),
            context, values,
        );
    }
    eval_with_native_bridge_scope(declaring_class, context, || {
        values.property_get(object, property)
    })
}

/// Writes a native property through its setter, preserving raw storage for backed properties without one.
pub(in crate::interpreter) fn eval_native_property_set_authorized(
    object: RuntimeCellHandle,
    declaring_class: &str,
    property: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(result) = eval_native_property_hook(
        object, declaring_class, property, Some(value), context, values,
    )? {
        return release_expr_result(result, context, values);
    }
    if native_property_is_virtual(declaring_class, property, values)? {
        return eval_throw_property_hook_readonly_error(declaring_class, property, context, values);
    }
    eval_with_native_bridge_scope(declaring_class, context, || {
        values.property_set(object, property, value)
    })
}

/// Invokes only metadata-marked hook methods, keeping inherited accessor scope and receiver identity.
fn eval_native_property_hook(
    object: RuntimeCellHandle,
    property_owner: &str,
    property: &str,
    assigned: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let method = if assigned.is_some() {
        property_hook_set_method(property)
    } else {
        property_hook_get_method(property)
    };
    let Some(flags) = values.reflection_method_flags(property_owner, &method)? else {
        return Ok(None);
    };
    if flags & EVAL_REFLECTION_METHOD_FLAG_PROPERTY_HOOK == 0 {
        return Ok(None);
    }
    if flags & (EVAL_REFLECTION_MEMBER_FLAG_STATIC | EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT) != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let hook_owner = values.reflection_method_declaring_class(property_owner, &method)?
        .unwrap_or_else(|| property_owner.to_string());
    let identity = values.object_identity(object)?;
    let called_class = if let Some(class) = context.dynamic_object_class(identity) {
        class.name().to_string()
    } else {
        eval_runtime_object_class_name(object, values)?
    };
    let arguments = positional_args(assigned.into_iter().map(RuntimeCellHandle::borrowed).collect());
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object, &hook_owner, &method, arguments, Some(&hook_owner), Some(&called_class), context, values,
    ).map(Some)
}

/// Returns whether the requested native property has no physical backing storage.
fn native_property_is_virtual(
    declaring_class: &str,
    property: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(values.reflection_property_flags(declaring_class, property)?
        .is_some_and(|flags| flags & EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL != 0))
}
