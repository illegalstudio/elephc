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

fn properties_compatible(left: &ClassProperty, right: &ClassProperty) -> bool {
    left.visibility == right.visibility
        && left.type_expr == right.type_expr
        && left.readonly == right.readonly
        && left.is_static == right.is_static
        && left.by_ref == right.by_ref
        && left.default == right.default
}
