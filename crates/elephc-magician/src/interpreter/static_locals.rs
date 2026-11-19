//! Purpose:
//! Coordinates persistent static cells, source contracts and activation bindings.
//!
//! Called from:
//! - Static declarations and Reflection initialization before the first call.
//!
//! Key details:
//! - Initialization succeeds before replacing an existing local binding.
//! - Persistent reference cells publish writes immediately and outlive detached names.

use super::*;

mod constants;

/// Prepares only silent constants for Reflection, leaving dynamic declarations uninitialized.
pub(super) fn initialize_constant(function: &str, name: &str, init: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(value) = context.static_local(function, name) { return Ok(Some(value)); }
    let producer = local_producer_contract(init, context, scope);
    let Some(value) = constants::prepare(init, context, values)? else { return Ok(None); };
    publish(function, name, value, producer, context, values).map(Some)
}

/// Initializes a persistent cell once, preserving producer alternatives before effects.
pub(super) fn initialize(function: &str, name: &str, init: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = context.static_local(function, name) { return Ok(value); }
    let producer = local_producer_contract(init, context, scope);
    let value = eval_owned_expr(init, context, scope, values)?;
    publish(function, name, value, producer, context, values)
}

/// Consumes a prepared initializer owner and installs its original producer contract once.
fn publish(function: &str, name: &str, value: RuntimeCellHandle,
    producer: Option<EvalParameterType>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // A recursive initializer can complete the same static before this activation returns.
    if let Some(initialized) = context.static_local(function, name) {
        eval_release_value(context, values, value)?;
        return Ok(initialized);
    }
    let prepared = (|| {
        let origin = if context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals) {
            Some(producer.map(Ok).unwrap_or_else(|| local_binding_origins::value_contract(value, context, values))?)
        } else { None };
        let cell = if values.supports_persistent_references() { values.reference_new(value)? }
            else { values.retain(value)? };
        context.copy_array_metadata(value, cell);
        if let Some(origin) = &origin { context.bind_local_reference_types(cell, std::slice::from_ref(origin)); }
        if let Some(old) = context.initialize_static_local(function, name, cell, origin) {
            eval_release_value(context, values, old)?;
        }
        Ok(cell.borrowed())
    })();
    let released = eval_release_value(context, values, value);
    match (prepared, released) {
        (Err(status), _) | (Ok(_), Err(status)) => Err(status),
        (Ok(cell), Ok(())) => Ok(cell),
    }
}

/// Rebinds a reached declaration to its persistent cell without modifying prior aliases.
pub(super) fn bind(name: &str, init: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(function) = context.current_function().map(str::to_string) else {
        let mut producer = local_binding_origins::prepare_assignment(init, context, scope);
        let value = local_binding_origins::evaluate_assignment(init, name, &mut producer, context, scope, values)?;
        let replaced = store_local_with_origin(context, scope, name.to_string(), value, producer, values)?;
        retire_scope_replacements(replaced, context, values)?;
        return Ok(());
    };
    let cell = initialize(&function, name, init, context, scope, values)?;
    let owned = values.retain(cell)?;
    let detached = unset_scope_cell(context, scope, name);
    let old = match detached {
        Ok(old) => old,
        Err(status) => { let _ = eval_release_value(context, values, owned); return Err(status); }
    };
    let replaced = scope.set(name.to_string(), owned, ScopeCellOwnership::Owned);
    if let Some(origin) = context.static_local_origin(&function, name).cloned() {
        scope.infer_local_type(name.to_string(), origin);
    }
    scope.mark_static_binding(name);
    // Destructors and their catches observe the published static binding.
    let mut cleanup = Ok(());
    for old in old.into_iter().chain(replaced) {
        if let Err(status) = eval_release_value(context, values, old) { cleanup = Err(status); }
    }
    cleanup
}

/// Copies an unescaped static value or retains a reference already shared by PHP owners.
pub(super) fn snapshot(value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.is_reference(value)? && !values.reference_is_shared(value)? {
        copy_scope_value(value, context, values)
    } else { values.retain(value) }
}
