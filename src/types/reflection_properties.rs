//! Purpose:
//! Identifies the builtin Reflection ancestry that handles public metadata writes.
//!
//! Called from:
//! - Property assignment checking and EIR property-write lowering.
//!
//! Key details:
//! - User redeclarations retain the builtin ancestor's handler.
//! - Internal-origin metadata prevents same-named user classes from acquiring it.

use std::collections::HashMap;

use super::ClassInfo;

/// Returns whether this class inherits Reflection's direct public mirror write handler.
pub(crate) fn class_has_public_mirror_write_handler(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    property: &str,
) -> bool {
    if !matches!(property, "name" | "class") {
        return false;
    }
    const OWNERS: &[&str] = &[
        "ReflectionAttribute", "ReflectionClass", "ReflectionObject", "ReflectionEnum",
        "ReflectionFunction", "ReflectionMethod", "ReflectionProperty", "ReflectionParameter",
        "ReflectionClassConstant", "ReflectionEnumUnitCase", "ReflectionEnumBackedCase",
    ];
    let mut current = class_name.trim_start_matches('\\');
    for _ in 0..=classes.len() {
        let Some(info) = classes.get(current) else { return false; };
        if info.is_internal && OWNERS.contains(&current) {
            return true;
        }
        let Some(parent) = info.parent.as_deref() else { return false; };
        current = parent.trim_start_matches('\\');
    }
    false
}
