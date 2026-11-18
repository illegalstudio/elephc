//! Purpose:
//! Publishes ordinary empty property containers before key checks and callbacks.
//!
//! Called from:
//! - Property dimension writes after selecting and authorizing a backing slot.
//!
//! Key details:
//! - Captured writes own a separate value and preserve a callback's replacement.
//! - Accessor and attached native storage keep their existing write-selection paths.

use super::*;

/// An already authorized ordinary backing slot whose parent can become an array.
pub(super) enum ArrayParentPlace<'a> {
    Instance(RuntimeCellHandle, &'a str),
    Static(&'a str, &'a str),
}

impl ArrayParentPlace<'_> {
    /// Publishes an independent value through the slot's existing declaration checks.
    fn publish(&self, value: RuntimeCellHandle, context: &mut ElephcEvalContext,
        values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
        match self {
            Self::Instance(object, property) => eval_property_set_result(*object, property, value, context, values),
            Self::Static(class, property) => eval_static_property_set_result(class, property, value, context, values),
        }
    }

    /// Roots the slot's current cell so a callback cannot recycle its identity.
    fn capture(&self, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
        values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
        let current = match self {
            Self::Instance(object, property) => eval_property_get_result(*object, property, context, values)?,
            Self::Static(class, property) => eval_static_property_get_result(class, property, context, values)?,
        };
        let current = if current.is_borrowed() { values.retain(current)? } else { current };
        owners.push(current);
        Ok(current)
    }
}

/// Returns a captured writable value and whether its ordinary backing slot remains attached.
pub(super) fn prepare_empty_array_parent(
    place: ArrayParentPlace<'_>, current: RuntimeCellHandle, append: bool,
    owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, bool), EvalStatus> {
    let tag = values.type_tag(current)?;
    let was_false = tag == EVAL_TAG_BOOL && !values.truthy(current)?;
    if tag != EVAL_TAG_NULL && !was_false { return Ok((current, true)); }
    let empty = eval_array_autovivification(Some(current), append, context, values)?;
    owners.push(empty);
    place.publish(empty, context, values)?;
    if !was_false { return Ok((empty, true)); }
    let captured = values.copy_value(empty)?;
    owners.push(captured);
    let before = place.capture(owners, context, values)?;
    eval_false_array_parent_diagnostic(true, context, values)?;
    let after = place.capture(owners, context, values)?;
    Ok((captured, before == after))
}
