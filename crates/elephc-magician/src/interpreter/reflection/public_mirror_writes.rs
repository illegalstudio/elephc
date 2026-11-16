//! Purpose:
//! Preserves PHP's direct-write handler for public Reflection metadata mirrors in eval.
//!
//! Called from:
//! - Plain instance-property assignment statement dispatch after receiver and RHS evaluation.
//!
//! Key details:
//! - Compound updates and reference writeback bypass this handler, as they do in PHP.
//! - Declared instance slots on builtin Reflection descendants use the runtime class in errors.

use super::*;

/// Rejects an accessible declared `name` or `class` slot on a builtin Reflection descendant.
pub(in crate::interpreter) fn validate_reflection_public_mirror_direct_write(
    object: RuntimeCellHandle,
    property: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !matches!(property, "name" | "class") || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(());
    }
    let runtime_class = eval_reflection_object_class_name(object, context, values)?;
    // Parent metadata is already registered for AOT and eval classes. Ordinary objects
    // must not pay repeated runtime ancestry and property-metadata calls for common names.
    let owner = std::iter::once(runtime_class.clone())
        .chain(context.class_parent_names(&runtime_class))
        .find(|name| reflection_owner_kind(name).is_some()
            || name.eq_ignore_ascii_case("ReflectionAttribute"));
    let Some(owner) = owner else { return Ok(()); };
    if !eval_reflection_aot_class_flags(&owner, values)?
        .is_some_and(|(flags, _)| flags & EVAL_REFLECTION_CLASS_FLAG_INTERNAL != 0)
    {
        return Ok(());
    }
    let declared = if let Some((owner, slot)) = context.class_property(&runtime_class, property) {
        Some((owner, slot.visibility(), slot.write_visibility(), slot.is_static()))
    } else {
        let exact = eval_reflection_aot_property_access_metadata(&runtime_class, property, values)?;
        if exact.is_some() {
            exact
        } else if let Some(parent) = context.class_native_parent_name(&runtime_class) {
            eval_reflection_aot_property_access_metadata(&parent, property, values)?
        } else {
            None
        }
    };
    let Some((owner, _, write_visibility, false)) = declared else {
        return Ok(());
    };
    if validate_eval_member_access(&owner, write_visibility, context).is_err() {
        return Ok(());
    }
    eval_throw_reflection_exception(
        &format!("Cannot set read-only property {runtime_class}::${property}"),
        context,
        values,
    ).map(|_| ())
}
