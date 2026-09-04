//! Purpose:
//! Validates and indexes the shared class and constant catalogs, mirroring what
//! `crate::registry` does for function contracts.
//!
//! Called from:
//! - Compiler and Magician registries when proving every catalogued class-like and constant
//!   is provided, and when resolving a predefined constant's value.
//! - Documentation exporters that enumerate the shared PHP surface.
//!
//! Key details:
//! - Duplicate names, duplicate IDs, non-canonical ordering, and ID/name mismatches fail before
//!   either catalog is exposed.
//! - Class lookups are case-insensitive like PHP; constant lookups are case-sensitive.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{BuiltinId, ClassContract, ConstantContract};

/// Returns every shared class-like contract in canonical (lowercase) name order.
pub fn classes() -> &'static [ClassContract] {
    static CLASSES: OnceLock<Vec<ClassContract>> = OnceLock::new();
    CLASSES
        .get_or_init(|| {
            let mut classes = Vec::new();
            classes.extend_from_slice(crate::catalog_classes::CLASSES);
            classes.sort_unstable_by_key(|class| class.canonical_name());
            classes
        })
        .as_slice()
}

/// Returns every shared global constant contract in name order.
pub fn constants() -> &'static [ConstantContract] {
    static CONSTANTS: OnceLock<Vec<ConstantContract>> = OnceLock::new();
    CONSTANTS
        .get_or_init(|| {
            let mut constants = Vec::new();
            constants.extend_from_slice(crate::catalog_constants::CONSTANTS);
            constants.extend_from_slice(crate::catalog_constants_curl::CURL_CONSTANTS);
            constants.sort_unstable_by_key(|constant| constant.name);
            constants
        })
        .as_slice()
}

/// Finds one class-like contract by PHP-style case-insensitive name.
pub fn lookup_class(name: &str) -> Option<&'static ClassContract> {
    let canonical = name.trim_start_matches('\\').to_ascii_lowercase();
    class_index().get(canonical.as_str()).copied()
}

/// Finds one global constant contract by exact name (a leading `\` is tolerated).
pub fn lookup_constant(name: &str) -> Option<&'static ConstantContract> {
    constant_index().get(name.trim_start_matches('\\')).copied()
}

fn class_index() -> &'static HashMap<String, &'static ClassContract> {
    static INDEX: OnceLock<HashMap<String, &'static ClassContract>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut by_name = HashMap::with_capacity(classes().len());
        let mut by_id: HashMap<BuiltinId, &ClassContract> = HashMap::new();
        let mut previous: Option<String> = None;
        for class in classes() {
            let canonical = class.canonical_name();
            assert!(!class.name.is_empty(), "class contract names cannot be empty");
            assert!(
                !class.name.starts_with('\\'),
                "class contract name must not have a leading namespace separator: {}",
                class.name
            );
            if let Some(previous) = &previous {
                assert!(
                    *previous < canonical,
                    "class contracts must be strictly sorted: {previous} before {canonical}"
                );
            }
            assert_eq!(
                class.id,
                BuiltinId::from_canonical_name(&canonical),
                "class contract ID does not match canonical name: {}",
                class.name
            );
            if let Some(existing) = by_id.insert(class.id, class) {
                panic!(
                    "class contract ID collision between {} and {}",
                    existing.name, class.name
                );
            }
            assert!(
                by_name.insert(canonical.clone(), class).is_none(),
                "duplicate class contract name: {}",
                class.name
            );
            previous = Some(canonical);
        }
        by_name
    })
}

fn constant_index() -> &'static HashMap<&'static str, &'static ConstantContract> {
    static INDEX: OnceLock<HashMap<&'static str, &'static ConstantContract>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut by_name = HashMap::with_capacity(constants().len());
        let mut by_id: HashMap<BuiltinId, &ConstantContract> = HashMap::new();
        let mut previous: Option<&str> = None;
        for constant in constants() {
            assert!(!constant.name.is_empty(), "constant contract names cannot be empty");
            assert!(
                !constant.name.starts_with('\\'),
                "constant contract name must not have a leading namespace separator: {}",
                constant.name
            );
            if let Some(previous) = previous {
                assert!(
                    previous < constant.name,
                    "constant contracts must be strictly sorted: {previous} before {}",
                    constant.name
                );
            }
            assert_eq!(
                constant.id,
                BuiltinId::from_canonical_name(constant.name),
                "constant contract ID does not match its name: {}",
                constant.name
            );
            if let Some(existing) = by_id.insert(constant.id, constant) {
                panic!(
                    "constant contract ID collision between {} and {}",
                    existing.name, constant.name
                );
            }
            assert!(
                by_name.insert(constant.name, constant).is_none(),
                "duplicate constant contract name: {}",
                constant.name
            );
            previous = Some(constant.name);
        }
        by_name
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies both catalogs validate and resolve representative names.
    #[test]
    fn class_and_constant_catalogs_validate_and_resolve() {
        assert!(!classes().is_empty());
        assert!(!constants().is_empty());
        assert_eq!(
            lookup_class("\\arrayiterator").map(|class| class.name),
            Some("ArrayIterator")
        );
        assert_eq!(
            lookup_constant("JSON_PRETTY_PRINT").map(|constant| constant.name),
            Some("JSON_PRETTY_PRINT")
        );
        assert!(lookup_constant("json_pretty_print").is_none());
        for class in classes() {
            assert!(std::ptr::eq(
                lookup_class(class.name).expect("class name resolves"),
                class
            ));
        }
        for constant in constants() {
            assert!(std::ptr::eq(
                lookup_constant(constant.name).expect("constant name resolves"),
                constant
            ));
        }
    }
}
