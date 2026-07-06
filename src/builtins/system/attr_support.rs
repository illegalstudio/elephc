//! Purpose:
//! Shared helper functions for the class-attribute reflection builtins
//! (`class_attribute_names`, `class_attribute_args`, `class_get_attributes`).
//! Relocated from `src/types/checker/builtins/system.rs` into the builtin
//! registry home area so all three attribute homes can import them from one place.
//!
//! Called from:
//! - `crate::builtins::system::class_attribute_names` (check hook)
//! - `crate::builtins::system::class_attribute_args` (check hook)
//! - `crate::builtins::system::class_get_attributes` (check hook)
//!
//! Key details:
//! - `resolve_class_name` performs a case-insensitive PHP-symbol-key lookup.
//! - The two `*_unsupported` helpers inspect class attribute metadata to detect
//!   features that the flat helper builtins cannot faithfully represent.

use crate::names::php_symbol_key;
use crate::types::checker::Checker;

/// Resolves a class name to its canonical key in the checker's class table.
///
/// Returns `Some(canonical_name)` if the class exists, `None` otherwise.
/// The lookup is case-insensitive per PHP rules.
pub(crate) fn resolve_class_name<'a>(checker: &'a Checker, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    checker
        .classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Returns `true` if the named attribute on the class uses argument metadata
/// that the compiler does not yet support (i.e., `attribute_args` slot is `None`).
pub(crate) fn class_attribute_args_unsupported(
    checker: &Checker,
    class_name: &str,
    attr_name: &str,
) -> bool {
    let Some(resolved_class) = resolve_class_name(checker, class_name) else {
        return false;
    };
    let Some(class_info) = checker.classes.get(resolved_class) else {
        return false;
    };
    let attr_key = php_symbol_key(attr_name.trim_start_matches('\\'));
    class_info
        .attribute_names
        .iter()
        .enumerate()
        .find(|(_, name)| php_symbol_key(name.trim_start_matches('\\')) == attr_key)
        .is_some_and(|(idx, _)| match class_info.attribute_args.get(idx) {
            // The flat `class_attribute_args()` helper returns a positional
            // array of materialized scalars, so it cannot faithfully echo keyed
            // arguments (named arguments or associative arrays, at any depth) or
            // deferred symbolic references (global/class constants, enum cases).
            // Reject them and direct users to
            // `ReflectionClass::getAttributes()->getArguments()` instead.
            Some(Some(entries)) => attr_entries_unsupported_by_flat_helper(entries),
            _ => true,
        })
}

/// Returns true when the flat `class_attribute_args()` helper cannot faithfully
/// echo the captured entries: keyed arguments (named arguments or
/// associative-array keys, at any depth) would lose their keys, and deferred
/// symbolic references (global/class constants, enum cases) are not materialized
/// on this echo path. Both are supported through
/// `ReflectionClass::getAttributes()->getArguments()` instead.
pub(crate) fn attr_entries_unsupported_by_flat_helper(
    entries: &[crate::types::AttrArgEntry],
) -> bool {
    entries.iter().any(|entry| {
        entry.key.is_some()
            || matches!(
                &entry.value,
                crate::types::AttrArgValue::ConstRef(_)
                    | crate::types::AttrArgValue::ScopedConst(..)
            )
            || matches!(
                &entry.value,
                crate::types::AttrArgValue::Array(inner)
                    if attr_entries_unsupported_by_flat_helper(inner)
            )
    })
}

/// Returns `true` if the class has any attribute whose argument metadata is not
/// fully supported (slot count mismatch or any `None` slot in `attribute_args`).
pub(crate) fn class_get_attributes_unsupported(checker: &Checker, class_name: &str) -> bool {
    let Some(resolved_class) = resolve_class_name(checker, class_name) else {
        return false;
    };
    checker.classes.get(resolved_class).is_some_and(|class_info| {
        class_info.attribute_names.len() != class_info.attribute_args.len()
            || class_info.attribute_args.iter().any(Option::is_none)
    })
}
