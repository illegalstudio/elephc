//! Purpose:
//! Publishes ordinary empty property containers before key checks and callbacks.
//!
//! Called from:
//! - Property dimension writes after selecting and authorizing a backing slot.
//!
//! Key details:
//! - Plain stores mutate their captured value cell without replaying a removed parent slot.
//! - Accessor and attached native storage keep their existing write-selection paths.

use super::*;

/// An already authorized ordinary backing slot whose parent can become an array.
pub(super) enum ArrayParentPlace<'a> {
    Instance(RuntimeCellHandle, &'a str),
    Static(&'a str, &'a str),
}

/// Selects in-place opcode completion independently from ordinary snapshot writeback.
#[derive(Clone, Copy)]
pub(super) enum PreparedArrayParent {
    Snapshot(RuntimeCellHandle),
    Captured(RuntimeCellHandle),
    Detached(RuntimeCellHandle),
}

impl PreparedArrayParent {
    /// Returns the selected cell while the caller's operand list retains its owner.
    pub(super) fn value(self) -> RuntimeCellHandle {
        match self { Self::Snapshot(value) | Self::Captured(value) | Self::Detached(value) => value }
    }
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
        values: &mut impl RuntimeValueOps) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let present = match self {
            Self::Instance(object, property) => eval_property_isset_result(*object, property, context, values)?,
            Self::Static(class, property) => eval_static_property_isset_result(class, property, context, values)?,
        };
        if !present { return Ok(None); }
        let current = match self {
            Self::Instance(object, property) => eval_property_get_result(*object, property, context, values)?,
            Self::Static(class, property) => eval_static_property_get_result(class, property, context, values)?,
        };
        let current = if current.is_borrowed() { values.retain(current)? } else { current };
        owners.push(current);
        let value = values.retain_value_cell(current)?;
        owners.push(value);
        Ok(Some(value))
    }
}

/// Publishes empty storage and captures plain stores before a callback can remove their parent.
pub(super) fn prepare_empty_array_parent(
    place: ArrayParentPlace<'_>, current: RuntimeCellHandle, append: bool, update: bool,
    owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<PreparedArrayParent, EvalStatus> {
    let tag = values.type_tag(current)?;
    let was_false = tag == EVAL_TAG_BOOL && !values.truthy(current)?;
    if tag != EVAL_TAG_NULL && !was_false { return Ok(PreparedArrayParent::Snapshot(current)); }
    let empty = eval_array_autovivification(Some(current), append, context, values)?;
    owners.push(empty);
    place.publish(empty, context, values)?;
    if !was_false { return Ok(PreparedArrayParent::Snapshot(empty)); }
    if !update {
        let captured = place.capture(owners, context, values)?.ok_or(EvalStatus::RuntimeFatal)?;
        eval_false_array_parent_diagnostic(true, context, values)?;
        return Ok(PreparedArrayParent::Captured(captured));
    }
    let captured = values.copy_value(empty)?;
    owners.push(captured);
    let before = place.capture(owners, context, values)?;
    eval_false_array_parent_diagnostic(true, context, values)?;
    let after = place.capture(owners, context, values)?;
    Ok(if before == after { PreparedArrayParent::Snapshot(captured) }
        else { PreparedArrayParent::Detached(captured) })
}
