//! Purpose:
//! Recognizes unresolved direct function calls that PHP resolves only when execution reaches them.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution::call::Checker::check_function_call()`.
//! - `crate::ir_lower::expr::late_bound_call::lower_late_bound_undefined_call()`.
//!
//! Key details:
//! - Unresolved names remain absent from the builtin catalog, so `function_exists()` stays false.
//! - Executed calls become PHP-style catchable `Error` throws during EIR lowering.
//! - Extension-only builtins disabled by strict-PHP mode keep their explicit compile diagnostic.

/// Returns whether an unresolved canonical name may be deferred to PHP-style runtime lookup.
pub(crate) fn is_late_bound_undefined_function(canonical_name: &str) -> bool {
    let bare = canonical_name
        .rsplit('\\')
        .next()
        .unwrap_or(canonical_name);
    if bare.is_empty() {
        return false;
    }
    let key = crate::names::php_symbol_key(bare);
    !crate::types::checker::builtins::strict_php_hidden_builtin(&key)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for generic late-bound function matching.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Bare and namespaced unresolved names defer uniformly; malformed empty names do not.

    use super::*;

    /// Accepts arbitrary bare and namespaced unresolved function names.
    #[test]
    fn matches_unresolved_names_generically() {
        assert!(is_late_bound_undefined_function("missing_function"));
        assert!(is_late_bound_undefined_function("Vendor\\Package\\missing_function"));
    }

    /// Rejects an empty terminal function component.
    #[test]
    fn rejects_empty_function_name() {
        assert!(!is_late_bound_undefined_function(""));
    }
}
