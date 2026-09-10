//! Purpose:
//! Coordinates INI mutation lifetimes and short state borrows around reentrant diagnostics.
//!
//! Called from:
//! - Pure startup/tests and the protected native INI adapter using the same handlers.
//!
//! Key details:
//! - Old return values and raw storage identities are captured before warning callbacks.
//! - Restores consult the live original slot after callbacks, as Zend's restore handler does.

use super::*;

/// Accesses state for one non-callback operation, supporting owned and request-cell callers.
pub(super) trait Access {
    /// Releases the mutable state access before returning control to any diagnostic callback.
    fn with<T>(&mut self, action: impl FnOnce(&mut State) -> T) -> T;

    /// Updates an attached regex session after encoding diagnostics; pure state callers retain startup metadata only.
    fn configure_regex(&mut self, _name: &[u8]) {}
}

impl Access for &mut State {
    /// Applies one state operation directly when no PHP callbacks can access this state.
    fn with<T>(&mut self, action: impl FnOnce(&mut State) -> T) -> T { action(self) }
}

impl Access for &RefCell<State> {
    /// Keeps the request borrow scoped to one state operation, never a surrounding callback.
    fn with<T>(&mut self, action: impl FnOnce(&mut State) -> T) -> T { action(&mut self.borrow_mut()) }
}

/// Changes one directive and publishes its raw value only if a callback did not replace the slot.
pub(super) fn set(state: &mut impl Access, name: &[u8], value: IniString,
    validate: &mut impl FnMut(&[u8]) -> Result<(), MimeRegexError>, emit: &mut impl FnMut(Diagnostic)) -> IniUpdate {
    let mut result = IniUpdate { accepted: false, previous: None, diagnostics: Vec::new() };
    let Some(key) = catalog::lookup(name) else { return result; };
    if state.with(|state| state.ini.entries[key as usize].access & 1 == 0) { return result; }
    let previous = state.with(|state| {
        let entry = &mut state.ini.entries[key as usize];
        entry.mark_modified();
        entry.local.clone()
    });
    let returned = previous.as_ref().map_or_else(|| IniString::interned(b""), IniString::scalar_result);
    result.accepted = handlers::apply(state, key, Some(&value), "ini_set", validate, emit);
    if result.accepted {
        result.previous = Some(returned);
        state.with(|state| {
            let entry = &mut state.ini.entries[key as usize];
            let unchanged = match (&previous, &entry.local) {
                (None, None) => true, (Some(before), Some(after)) => before.same_identity(after), _ => false,
            };
            if unchanged { entry.local = Some(value); }
        });
    }
    result
}

/// Runs a restore handler and consumes its live original slot only when that handler succeeds.
pub(super) fn restore(state: &mut impl Access, name: &[u8],
    validate: &mut impl FnMut(&[u8]) -> Result<(), MimeRegexError>, emit: &mut impl FnMut(Diagnostic)) -> IniUpdate {
    let mut result = IniUpdate { accepted: false, previous: None, diagnostics: Vec::new() };
    let Some(key) = catalog::lookup(name) else { return result; };
    if state.with(|state| state.ini.entries[key as usize].access & 1 == 0) { return result; }
    let (modified, original) = state.with(|state| {
        let entry = &state.ini.entries[key as usize];
        (entry.modified, entry.original.clone())
    });
    if !modified { result.accepted = true; return result; }
    result.accepted = handlers::apply(state, key, original.as_deref(), "ini_restore", validate, emit);
    if result.accepted {
        state.with(|state| {
            let entry = &mut state.ini.entries[key as usize];
            entry.local = entry.original.take();
            entry.access = std::mem::take(&mut entry.original_access);
            entry.modified = false;
        });
    }
    result
}
