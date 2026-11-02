//! Purpose:
//! Resolves authorized native static-property aliases without copying their current values.
//!
//! Called from:
//! - By-reference eval call argument staging after the ordinary static read succeeds.
//!
//! Key details:
//! - Eval-owned slots and explicit eval aliases retain their existing reference targets.
//! - Native access checks precede switching to the declaring class's bridge scope.

use super::*;

/// Exports a native static cell only when eval's current property resolution selects that storage.
pub(in crate::interpreter) fn eval_native_static_property_reference_after_read(
    class_name: &str, property: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if context.class_property(&class_name, property).is_some() { return Ok(None); }
    let metadata_class = if eval_static_member_context_owns_class(&class_name, context) {
        let Some(parent) = context.class_native_parent_name(&class_name) else { return Ok(None); };
        parent
    } else { class_name.clone() };
    let Some((owner, visibility, _, true)) = eval_reflection_aot_static_property_access_metadata(
        &metadata_class, property, context, values,
    )? else { return Ok(None); };
    if context.static_property_alias(&owner, property).is_some() { return Ok(None); }
    if validate_eval_member_access(&owner, visibility, context).is_err() {
        return eval_throw_property_access_error(&owner, property, visibility, context, values);
    }
    eval_with_native_bridge_scope(&owner, context, || values.static_property_reference(&owner, property))
}
