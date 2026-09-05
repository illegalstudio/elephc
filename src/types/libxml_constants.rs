//! Purpose:
//! Defines the `LIBXML_*` integer constants Termwind and other HTML/XML callers
//! pass to `DOMDocument::loadHTML()`.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//! - `crate::name_resolver::names` so an unqualified `LIBXML_*` name inside a
//!   namespace falls back to the global constant the way PHP does.
//!
//! Key details:
//! - Values match php-src `ext/libxml` / libxml2 exactly so flag arithmetic
//!   (`LIBXML_NOERROR | LIBXML_NOBLANKS | …`) folds identically to PHP.
//! - This table is the Termwind-facing subset, not the full libxml constant
//!   surface. Draft PR #654 (`feat/php-dom-compliance`) owns complete PHP 8.5
//!   libxml/DOM parity; keep these integers stable so that work can absorb them.

/// Tuple of `(name, value)` pairs for the Termwind-facing `LIBXML_*` flags.
///
/// Example entries: `("LIBXML_NOERROR", 32)`, `("LIBXML_NOXMLDECL", 2)`.
pub(crate) const LIBXML_INT_CONSTANTS: &[(&str, i64)] = &[
    // `XML_SAVE_NO_DECL` — omit the XML declaration from `saveXML()`. Also
    // accepted as a `loadHTML()` flag (Termwind ORs it in); parse ignores it.
    ("LIBXML_NOXMLDECL", 2),
    // `HTML_PARSE_NODEFDTD` — do not add a default doctype.
    ("LIBXML_HTML_NODEFDTD", 4),
    // `XML_PARSE_NOERROR` — suppress parser error reports.
    ("LIBXML_NOERROR", 32),
    // `XML_PARSE_NOBLANKS` — drop whitespace-only text nodes.
    ("LIBXML_NOBLANKS", 256),
    // `XML_PARSE_COMPACT` — compact small text nodes (ignored by the subset parser).
    ("LIBXML_COMPACT", 65536),
];

#[cfg(test)]
mod tests {
    use super::LIBXML_INT_CONSTANTS;

    /// Looks up one `LIBXML_*` constant by name.
    fn value(name: &str) -> i64 {
        LIBXML_INT_CONSTANTS
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("{name} defined"))
            .1
    }

    /// Verifies Termwind's `loadHTML()` flag integers match php-src / libxml2.
    #[test]
    fn test_termwind_libxml_flag_values() {
        assert_eq!(value("LIBXML_NOXMLDECL"), 2);
        assert_eq!(value("LIBXML_HTML_NODEFDTD"), 4);
        assert_eq!(value("LIBXML_NOERROR"), 32);
        assert_eq!(value("LIBXML_NOBLANKS"), 256);
        assert_eq!(value("LIBXML_COMPACT"), 65536);
    }

    /// Verifies that no `LIBXML_*` constant name is declared twice.
    #[test]
    fn test_libxml_constants_have_unique_names() {
        let mut names: Vec<&str> = LIBXML_INT_CONSTANTS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LIBXML_INT_CONSTANTS.len());
    }
}
