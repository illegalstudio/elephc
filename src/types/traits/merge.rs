//! Purpose:
//! Implements trait merge logic for flattened class metadata.
//! Applies PHP trait composition rules before object inference and method checks consume class schemas.
//!
//! Called from:
//! - `crate::types::traits`
//!
//! Key details:
//! - Merge and validation rules must report conflicts early because downstream class metadata is treated as canonical.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, ClassProperty};
use crate::span::Span;

use super::validation::validate_direct_method_duplicates;

/// Merges `imported` trait properties with `local` class/trait properties.
/// Conflicts (same name, incompatible modifiers) are reported as errors.
/// If `replace_compatible_existing` is true, a compatible imported property
/// replaces an existing one (trait-precedence rule); otherwise the local property wins.
/// Returns the merged list in declaration order.
pub(super) fn merge_properties(
    imported: &[ClassProperty],
    local: &[ClassProperty],
    span: Span,
    owner_label: &str,
    replace_compatible_existing: bool,
) -> Result<Vec<ClassProperty>, CompileError> {
    let mut merged = imported.to_vec();
    for property in local {
        merge_property_into(
            &mut merged,
            property.clone(),
            span,
            owner_label,
            replace_compatible_existing,
        )?;
    }
    Ok(merged)
}

/// Appends `property` into `merged`, checking for duplicate name conflicts.
/// If a duplicate exists and is compatible with `replace_compatible_existing`,
/// the existing entry is replaced; otherwise a fatal error is emitted.
/// Visibility, type, hooks, readonly, static, abstract, by_ref, and default
/// must all match for two properties to be considered compatible.
pub(super) fn merge_property_into(
    merged: &mut Vec<ClassProperty>,
    property: ClassProperty,
    span: Span,
    owner_label: &str,
    replace_compatible_existing: bool,
) -> Result<(), CompileError> {
    if let Some(index) = merged
        .iter()
        .position(|existing| existing.name == property.name)
    {
        let existing = &merged[index];
        if existing.hooks.any() || property.hooks.any() {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} has incompatible duplicate hooked property '{}'",
                    owner_label, property.name
                ),
            ));
        }
        if properties_compatible(existing, &property) {
            if replace_compatible_existing {
                merged[index] = property;
            }
            return Ok(());
        }
        return Err(CompileError::new(
            span,
            &format!(
                "{} has incompatible duplicate property '{}'",
                owner_label, property.name
            ),
        ));
    }
    merged.push(property);
    Ok(())
}

/// Merges `imported` trait methods with `local` class/trait methods.
/// Validates that `local` has no duplicate method keys (name + is_static).
/// Imported methods with the same key as local are skipped (local wins).
/// Duplicate imported methods (same key from multiple traits) are reported as errors.
/// Returns the merged list: imported-first, then local.
pub(super) fn merge_methods(
    imported: Vec<ClassMethod>,
    local: &[ClassMethod],
    span: Span,
    owner_label: &str,
) -> Result<Vec<ClassMethod>, CompileError> {
    validate_direct_method_duplicates(local, span, owner_label)?;

    let mut local_keys = HashSet::new();
    for method in local {
        local_keys.insert((php_symbol_key(&method.name), method.is_static));
    }

    let mut merged = Vec::new();
    let mut seen_imported = HashSet::new();
    for imported_method in imported {
        let key = (php_symbol_key(&imported_method.name), imported_method.is_static);
        if local_keys.contains(&key) {
            continue;
        }
        if !seen_imported.insert(key.clone()) {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} imports duplicate trait method '{}'",
                    owner_label, imported_method.name
                ),
            ));
        }
        merged.push(imported_method);
    }

    merged.extend(local.iter().cloned());
    Ok(merged)
}

/// Appends each method in `incoming` to `existing`, tracking seen (name, is_static)
/// keys to prevent duplicate imports across different trait sources.
/// Errors if the same method key already exists in `existing` (trait conflict).
/// Used when accumulating methods from multiple traits within one class.
pub(super) fn merge_imported_method_set(
    existing: &mut Vec<ClassMethod>,
    incoming: Vec<ClassMethod>,
    span: Span,
    owner_label: &str,
) -> Result<(), CompileError> {
    let mut seen: HashSet<(String, bool)> = existing
        .iter()
        .map(|method| (php_symbol_key(&method.name), method.is_static))
        .collect();
    for method in incoming {
        let key = (php_symbol_key(&method.name), method.is_static);
        if !seen.insert(key) {
            return Err(CompileError::new(
                span,
                &format!("{} imports duplicate trait method '{}'", owner_label, method.name),
            ));
        }
        existing.push(method);
    }
    Ok(())
}

/// Returns true if `left` and `right` have matching visibility, type_expr,
/// hooks, readonly, static, abstract, by_ref, and default value.
/// Used by `merge_property_into` to determine whether two same-named properties
/// are compatible for trait composition (no conflict error).
fn properties_compatible(left: &ClassProperty, right: &ClassProperty) -> bool {
    left.visibility == right.visibility
        && left.type_expr == right.type_expr
        && left.hooks == right.hooks
        && left.readonly == right.readonly
        && left.is_static == right.is_static
        && left.is_abstract == right.is_abstract
        && left.by_ref == right.by_ref
        && left.default == right.default
}
