//! Purpose:
//! Selects ordinary property append storage without routing an element write through __set.
//!
//! Called from:
//! - The instance-property dimension writer after native XML selection.
//!
//! Key details:
//! - Existing dynamic entries bypass getters, including null-valued entries.
//! - Missing overloaded getters are explicitly refused rather than storing a value snapshot.
//! - Creation diagnostics precede storage selection so handler-created entries remain visible.

use super::*;

/// Handles dynamic and overloaded names while leaving accessible declarations on their normal path.
pub(super) fn select(
    object: RuntimeCellHandle, property: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else { return Ok(None); };
    let dynamic = context.dynamic_object_class(identity).map(|class| class.name().to_string());
    let class = match &dynamic {
        Some(class) => class.clone(), None => eval_runtime_object_class_name(object, values)?,
    };
    let mut inaccessible = false;
    if let Some((owner, declared)) = eval_dynamic_property_for_access(&class, property, context) {
        if declared.is_static() || declared.has_get_hook() || declared.has_set_hook()
            || validate_eval_member_access(&owner, declared.visibility(), context).is_ok() {
            return Ok(None);
        }
        inaccessible = true;
    } else {
        let native = if dynamic.is_some() {
            eval_dynamic_class_native_property_metadata(&class, property, context, values)?
        } else { eval_native_instance_property_metadata_for_access(&class, &class, property, context, values)? };
        if let Some((owner, read, _, is_static)) = native {
            if is_static || validate_eval_member_access(&owner, read, context).is_ok() { return Ok(None); }
            inaccessible = true;
        }
    }
    if !inaccessible && values.object_dynamic_property_exists(object, property)? {
        return Ok(Some(values.property_get(object, property)?));
    }
    let native = context.class_native_parent_name(&class).unwrap_or_else(|| class.clone());
    let getter = context.class_method(&class, "__get");
    let native_flags = if getter.is_none() { values.reflection_method_flags(&native, "__get")? } else { None };
    if getter.is_some() || native_flags.is_some() {
        return eval_throw_error("Unsupported property append through an overloaded __get()", context, values);
    }
    if inaccessible { return Ok(None); }
    prepare_creation(&class, property, context, values)?;
    let current = if values.object_dynamic_property_exists(object, property)? {
        values.property_get(object, property)?
    } else { values.null()? };
    Ok(Some(current))
}

/// Checks creation before reading a possibly handler-created dynamic entry.
fn prepare_creation(
    class: &str, property: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let readonly = match context.class(class) {
        Some(class) => class.is_readonly_class(),
        None => values.reflection_class_flags(class)?.is_some_and(|flags|
            flags & EVAL_REFLECTION_CLASS_FLAG_READONLY != 0),
    };
    if readonly || context.has_enum(class) || class.eq_ignore_ascii_case("Closure") {
        return eval_throw_error(&format!("Cannot create dynamic property {class}::${property}"), context, values);
    }
    if !instance_property_access::eval_class_allows_dynamic_properties(class, context) {
        eval_dispatch_php_error(&format!("Creation of dynamic property {class}::${property} is deprecated"),
            E_DEPRECATED, context, values)?;
    }
    Ok(())
}

/// Publishes a copied dynamic array directly, preserving the decision to bypass __set.
pub(super) fn write(
    object: RuntimeCellHandle, property: &str, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let class = match context.dynamic_object_class(identity) {
        Some(class) => class.name().to_string(), None => eval_runtime_object_class_name(object, values)?,
    };
    instance_property_access::eval_write_public_dynamic_property(
        object, identity, &class, property, value, context, values,
    )
}
