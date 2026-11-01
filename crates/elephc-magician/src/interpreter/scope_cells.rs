//! Purpose:
//! Provides scope-cell read, write, unset, global-alias, and reference-alias helpers for eval execution.
//!
//! Called from:
//! - `crate::interpreter::statements` and `crate::interpreter::expressions`.
//!
//! Key details:
//! - Global aliases redirect through `ElephcEvalContext` while local aliases stay in the materialized eval scope.
//! - PHP superglobals resolve through the same global scope from every scope, as if each scope
//!   had declared them `global`, whether or not an `eval()` call is executing: a function from
//!   a runtime-loaded file that compiled code calls back reads the request's `$_SERVER` too.
//! - Replaced owned cells are released by callers through existing scope APIs.
//! - Php's request superglobals behave like an automatic `global $name;` in EVERY scope, on every
//!   mention -- unlike an explicit `global` statement, this is not a one-time bind: a name in
//!   `REQUEST_SUPERGLOBAL_NAMES` redirects to the context's global scope even where no `global`
//!   statement was ever written and even after a local `unset()` of that name in this scope.

use super::*;

/// PHP's superglobals: every scope sees them without a `global` statement.
pub(in crate::interpreter) const EVAL_SUPERGLOBALS: [&str; 8] =
    ["_SERVER", "_GET", "_POST", "_COOKIE", "_FILES", "_ENV", "_REQUEST", "_SESSION"];

/// Returns the global scope a superglobal named `name` resolves through, when one is reachable.
///
/// `None` for any other name, or when no global scope is installed; the superglobal then
/// resolves in the current scope.
///
/// Not gated on an executing `eval()` call. It once was, because a callback could run in a
/// context retained past the frame whose global scope it pointed at; here that frame hands
/// the scope to the retained context instead of freeing it
/// (`ElephcEvalContext::retain_global_scope_for_request`), so the pointer outlives the frame.
/// The gate also left writes ungated (`update_scope_cell`), so a superglobal written outside
/// an eval call landed in the global scope and was then read back from the local one.
pub(in crate::interpreter) fn eval_superglobal_global_scope(
    context: &ElephcEvalContext,
    name: &str,
) -> Option<*mut ElephcEvalScope> {
    if !EVAL_SUPERGLOBALS.contains(&name) {
        return None;
    }
    context.global_scope_ptr()
}

/// Returns the global-scope name a variable resolves through, if any.
///
/// That is the target of an explicit `global` alias, or the variable itself when it is one of
/// PHP's superglobals and a global scope is reachable.
fn eval_global_target<'a>(
    context: &ElephcEvalContext,
    scope: &'a ElephcEvalScope,
    name: &'a str,
) -> Option<&'a str> {
    scope.global_alias_target(name).or_else(|| {
        eval_superglobal_global_scope(context, name).map(|_| name)
    })
}

/// Returns the distinct global scope a superglobal write must use, or `None` to stay local.
///
/// Explicit `global` aliases keep their stricter contract (a missing global scope is fatal),
/// so they are left to the callers' alias branches.
fn eval_superglobal_scope(
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    name: &str,
) -> Option<*mut ElephcEvalScope> {
    if scope.global_alias_target(name).is_some() {
        return None;
    }
    eval_superglobal_global_scope(context, name)
        .filter(|global| *global != scope as *const ElephcEvalScope as *mut ElephcEvalScope)
}

/// Returns the effective global-alias target for a name: an explicit `global` alias first, then a
/// superglobal's own name, matching php's implicit per-mention auto-global fetch.
///
/// Context-free form of `eval_global_target`, kept for the call sites that already hold the
/// scope but not an execution-state check.
pub(in crate::interpreter) fn effective_global_alias_target<'a>(
    scope: &'a ElephcEvalScope,
    name: &'a str,
) -> Option<&'a str> {
    scope
        .global_alias_target(name)
        .or_else(|| EVAL_SUPERGLOBALS.contains(&name).then_some(name))
}

/// Returns the eval-visible entry for a variable, following `global` aliases.
pub(in crate::interpreter) fn scope_entry(
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    name: &str,
) -> Option<ScopeEntry> {
    // A `static` reads from its slot every time, not from a copy taken when the activation
    // started: php keeps ONE slot per function, so a frame must see what a deeper frame wrote.
    if let Some(slot) = scope.static_alias_slot(name) {
        let slot = slot.to_string();
        // BORROWED, whatever the slot's own flags say. The slot OWNS the cell and keeps it after
        // the activation ends; the activation only borrows it. Handing the slot's entry back
        // verbatim reported `Owned`, so the activation's cleanup released a cell the slot still
        // held -- which the counting fixture catches as `reached count -1`.
        return unsafe { context.static_scope_ptr().as_ref() }
            .and_then(|statics| statics.visible_cell(&slot))
            .map(|cell| ScopeEntry::present(cell, ScopeCellOwnership::Borrowed, 0));
    }
    let Some(global_name) = eval_global_target(context, scope, name) else {
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
        .map(RuntimeCellHandle::borrowed)
}

/// Validates a variable write and consumes owned input on rejection, following global aliases.
pub(in crate::interpreter) fn set_scope_cell(
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    name: impl Into<String>,
    cell: RuntimeCellHandle,
    ownership: ScopeCellOwnership,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let name = name.into();
    if let Err(status) = validate_generic_local_contracts(scope, &name, cell, context, values) {
        if ownership == ScopeCellOwnership::Owned { let _ = eval_release_value(context, values, cell); }
        return Err(status);
    }
    let result = set_scope_cell_unchecked(context, scope, name, cell, ownership, values);
    if result.is_err() && ownership == ScopeCellOwnership::Owned { let _ = eval_release_value(context, values, cell); }
    result
}

/// Applies a validated write without repeating the destination's contract checks.
fn set_scope_cell_unchecked(
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, name: String,
    cell: RuntimeCellHandle, ownership: ScopeCellOwnership, values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if let Some(reference) = visible_scope_cell(context, scope, &name) {
        if values.is_reference(reference)? {
            if reference == cell { return Ok(Vec::new()); }
            let previous = values.reference_replace(reference, cell)?;
            context.copy_array_metadata(cell, reference);
            if let Some(global) = eval_superglobal_scope(context, scope, &name) {
                unsafe { global.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?.mark_reference_changed(&name);
            } else if let Some(global_name) = scope.global_alias_target(&name).map(str::to_string) {
                let global = context.global_scope_ptr().ok_or(EvalStatus::RuntimeFatal)?;
                if global == scope as *mut ElephcEvalScope {
                    scope.mark_reference_changed(&global_name);
                } else {
                    unsafe { global.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?.mark_reference_changed(&global_name);
                }
            } else {
                scope.mark_reference_changed(&name);
            }
            let mut released = vec![previous];
            if ownership == ScopeCellOwnership::Owned { released.push(cell); }
            return Ok(released);
        }
    }
    update_scope_cell(context, scope, name, |scope, name| {
        scope.set_respecting_references(name, cell, ownership)
    })
}

/// Consumes a fresh owner on success or failure, balancing identical local and global aliases.
pub(in crate::interpreter) fn set_owned_scope_cell(
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    name: String,
    cell: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if let Err(status) = validate_generic_local_contracts(scope, &name, cell, context, values) {
        let _ = eval_release_value(context, values, cell);
        return Err(status);
    }
    let result = (|| {
        if let Some(reference) = visible_scope_cell(context, scope, &name) {
            if values.is_reference(reference)? {
                let same_owner = reference == cell;
                let mut replaced = set_scope_cell_unchecked(context, scope, name, cell, ScopeCellOwnership::Owned, values)?;
                if same_owner { replaced.push(cell); }
                return Ok(replaced);
            }
        }
        update_scope_cell(context, scope, name, |scope, name| {
            scope.set_owned_respecting_references(name, cell)
        })
    })();
    if result.is_err() { let _ = eval_release_value(context, values, cell); }
    result
}

/// Applies a storage mutation to the actual scope behind a local or global alias.
fn update_scope_cell(
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    name: String,
    update: impl FnOnce(&mut ElephcEvalScope, String) -> Vec<RuntimeCellHandle>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    // A write to a `static` lands in the slot, so the next read -- in this frame, in a recursive
    // one, or in a later call -- sees it. The displaced cell is handed back for release exactly
    // as the scope would have done.
    if let Some(slot) = scope.static_alias_slot(&name).map(str::to_string) {
        let Some(statics) = (unsafe { context.static_scope_ptr().as_mut() }) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return Ok(update(statics, slot));
    }
    if let Some(global) = eval_superglobal_scope(context, scope, &name) {
        let global = unsafe { global.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
        return Ok(update(global, name));
    }
    if let Some(global_name) = effective_global_alias_target(scope, &name).map(str::to_string) {
        let Some(global_scope) = context.global_scope_ptr() else {
            // An explicit `global $x;` already validated a global scope exists before it could
            // ever mark this alias (see `execute_global_stmt`), so reaching here with no scope
            // pointer is a genuine fatal for it. A superglobal name carries no such guarantee --
            // nothing declared it -- so with no established global scope at all it degrades to an
            // ordinary local write rather than failing a statement php would never refuse.
            return if scope.global_alias_target(&name).is_some() {
                Err(EvalStatus::RuntimeFatal)
            } else {
                Ok(update(scope, name))
            };
        };
        let current_scope = scope as *mut ElephcEvalScope;
        if global_scope == current_scope {
            return Ok(update(scope, global_name));
        }
        let Some(global_scope) = (unsafe { global_scope.as_mut() }) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return Ok(update(global_scope, global_name));
    }
    Ok(update(scope, name))
}

/// Creates a PHP reference alias between two eval-visible variable names.
pub(in crate::interpreter) fn set_reference_alias(
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    target: &str,
    source: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if let Some(declared) = scope.local_type(target).cloned() {
        let current = visible_scope_cell(context, scope, source);
        let value = current.map_or_else(|| values.null(), Ok)?;
        let checked = validate_generic_local(&declared, target, value, context, values);
        if current.is_none() { let released = eval_release_value(context, values, value); checked.and(released)?; }
        else { checked?; }
    }
    let mut types = generic_local_contracts(scope, source, context);
    if let Some(declared) = scope.local_type(target).filter(|ty| !types.contains(ty)) { types.push(declared.clone()); }
    if !types.is_empty() {
        if let Some(reference) = eval_persistent_variable_reference(source, context, scope, values)? {
            context.bind_local_reference_types(reference, &types);
        }
    }
    let source_reference_target = scope.reference_target(source).cloned();
    // A superglobal source (`$session = &$_SESSION`) is a global alias too: the target names the
    // global itself. Only explicit `global` aliases were recognized, so the reference was made
    // between two LOCAL entries and died with the scope -- Symfony's session storage binds every
    // bag to an element of that local, and each later write to a bag failed.
    if let Some(global_name) = effective_global_alias_target(scope, source).map(str::to_string) {
        scope.mark_global_alias_to(target.to_string(), global_name);
        scope.remove_reference_target(target);
        return Ok(Vec::new());
    }
    if eval_superglobal_scope(context, scope, source).is_some() {
        scope.mark_global_alias_to(target.to_string(), source.to_string());
        return Ok(Vec::new());
    }
    let (cell, ownership) = scope_entry(context, scope, source)
        .filter(|entry| entry.flags().is_visible())
        .map_or_else(
            || values.null().map(|cell| (cell, ScopeCellOwnership::Owned)),
            |entry| Ok((entry.cell(), entry.flags().ownership)),
        )?;
    if values.is_reference(cell)? { context.bind_local_reference_types(cell, &types); }
    let replaced = scope.set_reference(target.to_string(), source.to_string(), cell, ownership);
    if let Some(source_reference_target) = source_reference_target {
        scope.set_reference_target(target.to_string(), source_reference_target);
    } else {
        scope.remove_reference_target(target);
    }
    Ok(replaced)
}

/// Unsets a variable, removing only the local alias when the name is a `global` alias.
///
/// A superglobal is different: PHP unsets the global itself from any scope, so it is unset in
/// the same global scope its reads and writes resolve through, and recorded so that no later
/// fragment re-creates it. The released cell, if this scope owned one, goes to the caller.
pub(in crate::interpreter) fn unset_scope_cell(
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    name: impl Into<String>,
) -> Option<RuntimeCellHandle> {
    let name = name.into();
    if scope.is_global_alias(&name) {
        scope.clear_global_alias(&name);
        return scope.unset_respecting_references(name);
    }
    note_superglobal_unset(&name);
    if let Some(global) = eval_superglobal_scope(context, scope, &name) {
        return unsafe { global.as_mut() }?.unset_respecting_references(name);
    }
    // `unset($x)` on a static breaks the local binding, exactly as it does for a global; the slot
    // itself keeps its value for the next call, which is what php does.
    scope.clear_static_alias(&name);
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

/// Releases activation-owned cells after return, throw, static, and by-reference values escape.
pub(in crate::interpreter) fn finish_activation_scope(
    scope: &mut ElephcEvalScope,
    result: Result<RuntimeCellHandle, EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut cleanup = Ok(());
    let drained = scope.drain_owned_cells();
    for value in drained {
        if let Err(status) = eval_release_value(context, values, value) {
            cleanup = Err(status);
        }
    }
    match (result, cleanup) {
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Err(status), _) => Err(status),
        (Ok(value), Ok(())) => Ok(value),
    }
}
