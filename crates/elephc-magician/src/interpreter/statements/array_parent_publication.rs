//! Purpose:
//! Publishes ordinary empty property containers at their PHP-observable mutation point.
//!
//! Called from:
//! - Property dimension writes after selecting and authorizing a backing slot.
//!
//! Key details:
//! - Plain stores mutate their captured value cell without replaying a removed parent slot.
//! - Accessor and attached native storage keep their existing write-selection paths.
//! - Exact compound keys reject before null/false promotion; plain stores remain publication-first.

use super::*;

/// An already authorized ordinary backing slot whose parent can become an array.
pub(super) enum ArrayParentPlace<'a> {
    Variable(&'a str),
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
    /// Dispatches a plain false-parent warning with deferred completion on its captured cell.
    pub(super) fn finish_write<V: RuntimeValueOps>(
        self, index: Option<RuntimeCellHandle>, context: &mut ElephcEvalContext, values: &mut V,
        write: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
    ) -> Result<(), EvalStatus> {
        super::false_array_completion::with_completion(matches!(self, Self::Captured(_)), index, context, values, write)
    }

    /// Returns the selected cell while the caller's operand list retains its owner.
    pub(super) fn value(self) -> RuntimeCellHandle {
        match self { Self::Snapshot(value) | Self::Captured(value) | Self::Detached(value) => value }
    }
}

impl ArrayParentPlace<'_> {
    /// Publishes an independent value through the slot's existing declaration checks.
    fn publish(&self, value: RuntimeCellHandle, context: &mut ElephcEvalContext,
        scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
        match self {
            Self::Variable(name) => {
                validate_generic_local_contracts(scope, name, value, context, values)?;
                write_back_owned_variable_ref_target(scope, name, value, context, values)
            }
            Self::Instance(object, property) => eval_property_set_result(*object, property, value, context, values),
            Self::Static(class, property) => eval_static_property_set_result(class, property, value, context, values),
        }
    }

    /// Roots the slot's current cell so a callback cannot recycle its identity.
    fn capture(&self, owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
        scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let present = match self {
            Self::Variable(name) => visible_scope_cell(context, scope, name).is_some(),
            Self::Instance(object, property) => eval_property_isset_result(*object, property, context, values)?,
            Self::Static(class, property) => eval_static_property_isset_result(class, property, context, values)?,
        };
        if !present { return Ok(None); }
        let current = match self {
            Self::Variable(name) => visible_scope_cell(context, scope, name).ok_or(EvalStatus::RuntimeFatal)?.borrowed(),
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
    place: ArrayParentPlace<'_>, current: RuntimeCellHandle, index: Option<RuntimeCellHandle>, update: bool,
    owners: &mut Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<PreparedArrayParent, EvalStatus> {
    checked_array_keys::before_update_parent_materialization(current, index, update, context, values)?;
    let tag = values.type_tag(current)?;
    let was_false = tag == EVAL_TAG_BOOL && !values.truthy(current)?;
    if tag != EVAL_TAG_NULL && !was_false { return Ok(PreparedArrayParent::Snapshot(current)); }
    let empty = eval_array_autovivification(Some(current), index.is_none(), context, values)?;
    owners.push(empty);
    place.publish(empty, context, scope, values)?;
    if !was_false { return Ok(PreparedArrayParent::Snapshot(empty)); }
    if !update {
        let captured = place.capture(owners, context, scope, values)?.ok_or(EvalStatus::RuntimeFatal)?;
        return Ok(PreparedArrayParent::Captured(captured));
    }
    let captured = values.copy_value(empty)?;
    owners.push(captured);
    let before = place.capture(owners, context, scope, values)?;
    eval_false_array_parent_diagnostic(true, context, values)?;
    let after = place.capture(owners, context, scope, values)?;
    Ok(if before == after { PreparedArrayParent::Snapshot(captured) }
        else { PreparedArrayParent::Detached(captured) })
}
