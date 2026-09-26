//! Purpose:
//! Promotes live eval variables and captures into runtime-owned PHP reference cells.
//!
//! Called from:
//! - Array literal reference binding and closure capture materialization.
//!
//! Key details:
//! - Local alias groups retain one scope owner, while arrays and closures retain their own owners.
//! - Live scope targets are resolved before a persistent binding escapes its creating activation.

use super::*;

/// Obtains stable reference storage for a local or captured variable while its scope is live.
pub(in crate::interpreter) fn eval_persistent_variable_reference(
    local: &str,
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !values.supports_persistent_references() { return Ok(None); }
    let target = scope.reference_target(local).cloned().unwrap_or_else(|| EvalReferenceTarget::Variable {
        scope: scope as *mut ElephcEvalScope, name: local.to_string(),
    });
    let reference = match persistent_reference_target(target)? {
        EvalReferenceTarget::Variable { scope: target_scope, name } => {
            promote_array_reference_variable(target_scope, &name, context, values)?
        },
        EvalReferenceTarget::Cell { cell } if values.is_reference(cell)? => cell,
        _ => return Ok(None),
    };
    if scope.visible_cell(local) != Some(reference) {
        for old in scope.set_respecting_references(local.to_string(), reference, ScopeCellOwnership::Borrowed) {
            values.release(old)?;
        }
    }
    Ok(Some(reference))
}
/// Promotes a live variable and its local alias group into one independently owned reference cell.
fn promote_array_reference_variable(
    scope: *mut ElephcEvalScope,
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let source = unsafe { scope.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
    if let Some(global_name) = source.global_alias_target(name) {
        let global_scope = context.global_scope_ptr().ok_or(EvalStatus::RuntimeFatal)?;
        if global_scope != scope || global_name != name {
            return promote_array_reference_variable(global_scope, global_name, context, values);
        }
    }
    let (value, ownership) = source.entry(name).filter(|entry| entry.flags().is_visible())
        .map(|entry| (entry.cell(), entry.flags().ownership))
        .map_or_else(|| values.null().map(|value| (value, ScopeCellOwnership::Owned)), Ok)?;
    if values.is_reference(value)? { return Ok(value); }
    let reference = values.reference_new(value)?;
    for old in source.set_reference(name.to_string(), name.to_string(), value, ownership) {
        values.release(old)?;
    }
    for old in source.set_respecting_references(name.to_string(), reference, ScopeCellOwnership::Owned) {
        values.release(old)?;
    }
    Ok(reference)
}

/// Follows captured reference targets before an array can outlive the current closure activation.
pub(in crate::interpreter) fn persistent_reference_target(mut target: EvalReferenceTarget) -> Result<EvalReferenceTarget, EvalStatus> {
    let mut seen = std::collections::HashSet::new();
    loop {
        match target {
            EvalReferenceTarget::Variable { scope, ref name } => {
                if !seen.insert((scope as usize, name.clone())) { return Err(EvalStatus::RuntimeFatal); }
                let source = unsafe { scope.as_ref() }.ok_or(EvalStatus::RuntimeFatal)?;
                let Some(parent) = source.reference_target(name).cloned() else { return Ok(target); };
                target = parent;
            },
            EvalReferenceTarget::ArrayElement { scope, array_name, index } => {
                let array_target = persistent_reference_target(EvalReferenceTarget::Variable { scope, name: array_name })?;
                return Ok(EvalReferenceTarget::NestedArrayElement { array_target: Box::new(array_target), index });
            },
            EvalReferenceTarget::NestedArrayElement { array_target, index } => {
                let array_target = persistent_reference_target(*array_target)?;
                return Ok(EvalReferenceTarget::NestedArrayElement { array_target: Box::new(array_target), index });
            },
            _ => return Ok(target),
        }
    }
}
