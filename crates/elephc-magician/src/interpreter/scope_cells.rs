//! Purpose:
//! Provides scope-cell read, write, unset, global-alias, and reference-alias helpers for eval execution.
//!
//! Called from:
//! - `crate::interpreter::statements` and `crate::interpreter::expressions`.
//!
//! Key details:
//! - Global aliases redirect through `ElephcEvalContext` while local aliases stay in the materialized eval scope.
//! - Replaced owned cells are released by callers through existing scope APIs.

use super::*;

/// Returns the eval-visible entry for a variable, following `global` aliases.
pub(in crate::interpreter) fn scope_entry(
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    name: &str,
) -> Option<ScopeEntry> {
    let Some(global_name) = scope.global_alias_target(name) else {
        return scope.entry(name);
    };
    let Some(global_scope) = context.global_scope_ptr() else {
        return scope.entry(name);
    };
    let current_scope = scope as *const ElephcEvalScope as *mut ElephcEvalScope;
    if global_scope == current_scope {
        return scope.entry(global_name);
    }
    unsafe {
        global_scope
            .as_ref()
            .and_then(|scope| scope.entry(global_name))
    }
}

/// Returns the eval-visible cell for a variable, following `global` aliases.
pub(in crate::interpreter) fn visible_scope_cell(
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    name: &str,
) -> Option<RuntimeCellHandle> {
    scope_entry(context, scope, name)
        .filter(|entry| entry.flags().is_visible())
        .map(ScopeEntry::cell)
}

/// Stores a variable cell, redirecting `global` aliases to the global scope.
pub(in crate::interpreter) fn set_scope_cell(
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    name: impl Into<String>,
    cell: RuntimeCellHandle,
    ownership: ScopeCellOwnership,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let name = name.into();
    if let Some(global_name) = scope.global_alias_target(&name).map(str::to_string) {
        let Some(global_scope) = context.global_scope_ptr() else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let current_scope = scope as *mut ElephcEvalScope;
        if global_scope == current_scope {
            return Ok(scope.set_respecting_references(global_name, cell, ownership));
        }
        let Some(global_scope) = (unsafe { global_scope.as_mut() }) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return Ok(global_scope.set_respecting_references(global_name, cell, ownership));
    }
    Ok(scope.set_respecting_references(name, cell, ownership))
}

/// Creates a PHP reference alias between two eval-visible variable names.
pub(in crate::interpreter) fn set_reference_alias(
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    target: &str,
    source: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if let Some(global_name) = scope.global_alias_target(source).map(str::to_string) {
        scope.mark_global_alias_to(target.to_string(), global_name);
        return Ok(Vec::new());
    }
    let (cell, ownership) = scope_entry(context, scope, source)
        .filter(|entry| entry.flags().is_visible())
        .map_or_else(
            || values.null().map(|cell| (cell, ScopeCellOwnership::Owned)),
            |entry| Ok((entry.cell(), entry.flags().ownership)),
        )?;
    Ok(scope.set_reference(target.to_string(), source.to_string(), cell, ownership))
}

/// Unsets a variable, removing only the local alias when the name is global.
pub(in crate::interpreter) fn unset_scope_cell(
    scope: &mut ElephcEvalScope,
    name: impl Into<String>,
) -> Option<RuntimeCellHandle> {
    let name = name.into();
    if scope.is_global_alias(&name) {
        scope.clear_global_alias(&name);
    }
    scope.unset_respecting_references(name)
}

/// Marks variables as aliases to the context global scope for later reads/writes.
pub(in crate::interpreter) fn execute_global_stmt(
    vars: &[String],
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
) -> Result<(), EvalStatus> {
    if context.global_scope_ptr().is_none() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for name in vars {
        scope.mark_global_alias(name.clone());
    }
    Ok(())
}
