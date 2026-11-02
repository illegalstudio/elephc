//! Purpose:
//! Provides default context construction and normalized registry-key helpers.
//!
//! Called from:
//! - Context registries and public `Default` construction.
//!
//! Key details:
//! - PHP class, method, property, constant, and enum-case names use their required casing rules.

use super::*;

impl Default for ElephcEvalContext {
    /// Creates the default process-level eval context.
    fn default() -> Self {
        Self::new()
    }
}

/// Normalizes PHP class names for the eval dynamic class registry.
pub(super) fn normalize_class_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Adds an external declaration name once while preserving PHP-visible spelling.
///
/// `shared` is the prefix this context inherited and cannot write to; `names` is its own tail.
/// The duplicate check reads both, and compares case-insensitively in place -- the previous
/// version allocated a normalized `String` for every candidate it walked past.
pub(super) fn push_external_declared_name(
    shared: &[String],
    names: &mut Vec<String>,
    name: &str,
) -> bool {
    let visible_name = name.trim_start_matches('\\');
    if visible_name.is_empty() {
        return false;
    }
    let already_declared = shared
        .iter()
        .chain(names.iter())
        .any(|existing| existing.trim_start_matches('\\').eq_ignore_ascii_case(visible_name));
    if !already_declared {
        names.push(visible_name.to_string());
    }
    true
}

/// Normalizes PHP enum case names for case-sensitive eval enum lookup.
pub(super) fn normalize_enum_case_name(name: &str) -> String {
    name.to_string()
}

/// Normalizes PHP method names for case-insensitive native metadata lookup.
pub(super) fn normalize_method_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Builds the folded native method metadata key used for eval argument binding.
pub(super) fn native_method_key(class_name: &str, method_name: &str) -> (String, String) {
    (
        normalize_class_name(class_name),
        normalize_method_name(method_name),
    )
}

/// Builds the folded native property metadata key used for eval reflection.
pub(super) fn native_property_key(class_name: &str, property_name: &str) -> (String, String) {
    (
        normalize_class_name(class_name),
        property_name.trim_start_matches('$').to_string(),
    )
}

/// Builds the case-sensitive native class-constant metadata key used for eval reflection.
pub(super) fn native_constant_key(class_name: &str, constant_name: &str) -> (String, String) {
    (
        normalize_class_name(class_name),
        constant_name.to_string(),
    )
}

/// Pushes a PHP class-like name once, preserving the first visible spelling.
pub(super) fn push_unique_class_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    let key = normalize_class_name(name);
    if seen.insert(key) {
        names.push(name.trim_start_matches('\\').to_string());
    }
}

/// Returns whether two PHP class-like names resolve to the same normalized spelling.
pub(super) fn same_class_name(left: &str, right: &str) -> bool {
    normalize_class_name(left) == normalize_class_name(right)
}

/// Returns the interfaces one PHP BUILTIN class-like name extends or implements, in PHP's order.
///
/// A builtin name carries no eval declaration, so a parent walk over eval metadata alone stops
/// at it: a class implementing `Iterator` was never seen as a `Traversable`, and a return type
/// narrowed to `Iterator` was refused where php accepts it.
/// The list is transitive because PHP reports it that way — measured with `php -n` 8.5.6,
/// `ReflectionClass('OuterIterator')->getInterfaceNames()` is `Iterator,Traversable`, and a class
/// implementing `Iterator` reports `Iterator,Traversable`.
///
/// `Generator` is a builtin CLASS rather than an interface and belongs here for the same
/// reason: it implements `Iterator`, so `getIterator(): Generator` is a legal narrowing of
/// `IteratorAggregate::getIterator(): Traversable`.
pub(super) fn builtin_class_like_parent_names(name: &str) -> &'static [&'static str] {
    let name = name.trim_start_matches('\\');
    if name.eq_ignore_ascii_case("Iterator") || name.eq_ignore_ascii_case("IteratorAggregate") {
        return &["Traversable"];
    }
    if name.eq_ignore_ascii_case("OuterIterator")
        || name.eq_ignore_ascii_case("RecursiveIterator")
        || name.eq_ignore_ascii_case("SeekableIterator")
        || name.eq_ignore_ascii_case("Generator")
    {
        return &["Iterator", "Traversable"];
    }
    &[]
}

/// Returns whether a class-like name is one of PHP's native enum marker interfaces.
pub(super) fn is_php_enum_marker_interface(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    name.eq_ignore_ascii_case("UnitEnum") || name.eq_ignore_ascii_case("BackedEnum")
}

/// Pushes a case-insensitive PHP method name once for ReflectionClass metadata.
pub(super) fn push_unique_method_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    let key = normalize_method_name(name);
    if seen.insert(key) {
        names.push(name.trim_start_matches('\\').to_string());
    }
}

/// Pushes a case-sensitive PHP property name once for ReflectionClass metadata.
pub(super) fn push_unique_property_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    if seen.insert(name.to_string()) {
        names.push(name.to_string());
    }
}

/// Pushes a case-sensitive PHP class constant name once for ReflectionClass metadata.
pub(super) fn push_unique_constant_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    if seen.insert(name.to_string()) {
        names.push(name.to_string());
    }
}

/// Normalizes PHP constant names for case-sensitive eval dynamic probes.
pub(super) fn normalize_constant_name(name: &str) -> String {
    name.trim_start_matches('\\').to_string()
}
