//! Purpose:
//! Records the GLOBAL function names an eager autoload file has already declared, so a bare call
//! inside a later-resolved namespaced file gets PHP's global fallback.
//!
//! Called from:
//! - `crate::autoload` after the eager `files` entries are loaded.
//! - `crate::name_resolver::symbols::canonical_function` as the last resolution step.
//!
//! Key details:
//! - Compile-scoped, like `crate::strict_php` and `crate::superglobals`: autoloaded files are
//!   name-resolved one at a time, deep inside the autoload pass, and threading a set through every
//!   caller would touch far more code than the fact is worth.

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// `php_symbol_key`s of global functions declared by eager autoload files.
    static EAGER_GLOBAL_FUNCTIONS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Replaces the recorded set. Called once, after the eager `files` entries are loaded and before
/// any class file is resolved — which is the ordering that makes this work at all.
pub(crate) fn set(names: HashSet<String>) {
    EAGER_GLOBAL_FUNCTIONS.with(|cell| *cell.borrow_mut() = names);
}

/// Returns the recorded set, for a thread that must answer `declares` like this one.
pub(crate) fn snapshot() -> HashSet<String> {
    EAGER_GLOBAL_FUNCTIONS.with(|cell| cell.borrow().clone())
}

/// Whether an eager file declared this GLOBAL function.
///
/// The name is the bare spelling as written at the call site. A qualified name is never a
/// candidate: PHP falls back to the global namespace only for an unqualified call.
pub(crate) fn declares(name: &str) -> bool {
    let bare = name.trim_start_matches('\\');
    if bare.contains('\\') {
        return false;
    }
    let key = crate::names::php_symbol_key(bare);
    EAGER_GLOBAL_FUNCTIONS.with(|cell| cell.borrow().contains(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qualified_name_never_takes_the_global_fallback() {
        set(HashSet::from(["trigger_deprecation".to_string()]));
        assert!(declares("trigger_deprecation"));
        assert!(declares("\\trigger_deprecation"));
        assert!(!declares("Symfony\\Component\\trigger_deprecation"));
        set(HashSet::new());
    }

    #[test]
    fn lookup_is_case_insensitive_as_php_function_names_are() {
        set(HashSet::from(["trigger_deprecation".to_string()]));
        assert!(declares("Trigger_Deprecation"));
        set(HashSet::new());
    }
}
