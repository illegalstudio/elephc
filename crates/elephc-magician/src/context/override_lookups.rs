//! Purpose:
//! Records the classes the inheritance checks of one class have to look up, while that class
//! is being bound before the first statement of its program.
//!
//! Called from:
//! - `crate::interpreter::return_type_compat`, which reports each lookup a check makes.
//! - `crate::interpreter::statements::early_binding`, which opens the record around one bind
//!   and decides from it whether PHP could have checked the class while compiling.
//!
//! Key details:
//! - php-src binds a child early only when every override can be checked with the classes
//!   that exist. A check that needs a class it cannot find is undecided, and the class is
//!   linked when its statement runs.
//! - The checks have no runtime handle, so they only record names. The caller, which can ask
//!   the runtime, decides which of them exist.
//! - Nothing is recorded outside an open record, so a class declared by its statement pays
//!   one `None` test per lookup.

use super::*;

/// The class names the override checks of one class looked up while it was bound early.
#[derive(Default)]
pub(crate) struct EvalOverrideLookups {
    /// Classes PHP has to find even when the check then succeeds: a type accepted as `object`.
    /// Each entry is one such type: a single class, or the members of an intersection, of
    /// which PHP needs to find only one.
    pub(crate) required: Vec<Vec<String>>,
    /// Classes PHP has to find to compare two different class names.
    pub(crate) compared: Vec<String>,
}

impl ElephcEvalContext {
    /// Opens the lookup record for the class about to be bound early.
    pub(crate) fn begin_override_lookups(&mut self) {
        *self.override_lookups.borrow_mut() = Some(EvalOverrideLookups::default());
    }

    /// Closes the lookup record and returns what the checks looked up.
    pub(crate) fn end_override_lookups(&mut self) -> EvalOverrideLookups {
        self.override_lookups.borrow_mut().take().unwrap_or_default()
    }

    /// Records that a check accepted a type as `object`, which PHP decides only once it has
    /// found one of the classes the type names: `names` is a single class, or every member of
    /// an intersection.
    pub(crate) fn note_required_override_lookup(&self, names: &[String]) {
        if let Some(lookups) = self.override_lookups.borrow_mut().as_mut() {
            lookups.required.push(names.to_vec());
        }
    }

    /// Records that a check compared two different class names, which PHP decides only once
    /// it has found both classes.
    pub(crate) fn note_compared_override_lookup(&self, subtype: &str, supertype: &str) {
        if let Some(lookups) = self.override_lookups.borrow_mut().as_mut() {
            lookups.compared.push(subtype.to_string());
            lookups.compared.push(supertype.to_string());
        }
    }

    /// Returns the types the open record says PHP has to find for a check that succeeded,
    /// each as its alternative class names. Empty when no record is open.
    pub(crate) fn required_override_lookups(&self) -> Vec<Vec<String>> {
        self.override_lookups.borrow().as_ref().map(|lookups| lookups.required.clone()).unwrap_or_default()
    }
}
