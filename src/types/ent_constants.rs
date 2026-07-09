//! Purpose:
//! Defines the `ENT_*` HTML-escaping flag constants exposed as PHP integer constants.
//! Keeps the htmlspecialchars()/htmlentities() flag constants in one shared table for
//! checker and codegen seeding.
//!
//! Called from:
//! - `crate::types::checker::driver::init`
//! - `crate::codegen::prescan`
//!
//! Key details:
//! - Values match PHP's ext/standard `ENT_*` constants (quote-style bits, doctype bits,
//!   and invalid-sequence handling bits) so flag arithmetic folds identically to PHP.

/// PHP `ENT_*` integer constants consumed by `htmlspecialchars()`/`htmlentities()` flags.
pub(crate) const ENT_INT_CONSTANTS: &[(&str, i64)] = &[
    ("ENT_QUOTES", 3),
    ("ENT_COMPAT", 2),
    ("ENT_NOQUOTES", 0),
    ("ENT_HTML401", 0),
    ("ENT_HTML5", 48),
    ("ENT_XHTML", 32),
    ("ENT_XML1", 16),
    ("ENT_SUBSTITUTE", 8),
    ("ENT_IGNORE", 4),
];

#[cfg(test)]
mod tests {
    use super::ENT_INT_CONSTANTS;

    /// Verifies that the quote-style constants keep PHP's ext/standard bit values
    /// (`ENT_QUOTES` is `ENT_COMPAT | 1`, i.e. both quote bits set).
    #[test]
    fn test_ent_quote_style_values() {
        let value = |name: &str| {
            ENT_INT_CONSTANTS
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("{name} defined"))
                .1
        };
        assert_eq!(value("ENT_QUOTES"), 3);
        assert_eq!(value("ENT_COMPAT"), 2);
        assert_eq!(value("ENT_NOQUOTES"), 0);
    }

    /// Verifies that no `ENT_*` constant name is declared twice.
    #[test]
    fn test_ent_constants_have_unique_names() {
        let mut names: Vec<&str> = ENT_INT_CONSTANTS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ENT_INT_CONSTANTS.len());
    }
}
