//! Purpose:
//! Implements trait merge logic for flattened class metadata.
//! Applies PHP trait composition rules before object inference and method checks consume class schemas.
//!
//! Called from:
//! - `crate::types::traits`
//!
//! Key details:
//! - Merge and validation rules must report conflicts early because downstream class metadata is treated as canonical.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassConst, ClassMethod, ClassProperty};
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
        // `resolve_trait_uses` already merged duplicate imports (requirements included), so a
        // repeat here is a flattening bug, reported as the duplicate it is.
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
    let mut seen: HashMap<(String, bool), usize> = existing
        .iter()
        .enumerate()
        .map(|(index, method)| ((php_symbol_key(&method.name), method.is_static), index))
        .collect();
    for method in incoming {
        let key = (php_symbol_key(&method.name), method.is_static);
        if let Some(&index) = seen.get(&key) {
            merge_duplicate_imported_method(&mut existing[index], method, span, owner_label)?;
            continue;
        }
        seen.insert(key, existing.len());
        existing.push(method);
    }
    Ok(())
}

/// Resolves a second imported method with the key of `existing`, as PHP does: an abstract trait
/// method is a requirement, not a definition, so it never conflicts. A concrete method replaces
/// an abstract one (`use NeedsInvalidate, ProvidesInvalidate;`), and a second abstract one adds
/// nothing, each once `check_trait_requirement` accepts the pair. Only two concrete methods
/// conflict.
fn merge_duplicate_imported_method(
    existing: &mut ClassMethod,
    incoming: ClassMethod,
    span: Span,
    owner_label: &str,
) -> Result<(), CompileError> {
    if incoming.is_abstract {
        return check_trait_requirement(&incoming, existing, span, owner_label);
    }
    if existing.is_abstract {
        check_trait_requirement(existing, &incoming, span, owner_label)?;
        *existing = incoming;
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!("{} imports duplicate trait method '{}'", owner_label, incoming.name),
    ))
}

/// Checks that `other`, another trait's method of the same name and kind, may stand for the
/// abstract trait method `required`: a concrete method that implements it, or a second abstract
/// declaration of it.
///
/// PHP checks that pair like an override (`Declaration of B::foo() must be compatible with
/// A::foo()`). Until trait flattening runs that check, only an identical signature is accepted
/// here: the same parameter types, by-reference flags, optionality and variadic, and the same
/// return type and by-reference return. Visibility is not compared, as PHP does not compare it.
/// Anything else is rejected, as it was before abstract trait methods merged at all.
pub(super) fn check_trait_requirement(
    required: &ClassMethod,
    other: &ClassMethod,
    span: Span,
    owner_label: &str,
) -> Result<(), CompileError> {
    let same_params = required.params.len() == other.params.len()
        && required.params.iter().zip(&other.params).all(|(left, right)| {
            left.1 == right.1 && left.3 == right.3 && left.2.is_some() == right.2.is_some()
        });
    let same_signature = same_params
        && required.variadic.is_some() == other.variadic.is_some()
        && required.variadic_by_ref == other.variadic_by_ref
        && required.variadic_type == other.variadic_type
        && required.return_type == other.return_type
        && required.by_ref_return == other.by_ref_return;
    if same_signature {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!(
            "{} imports trait method '{}' with a signature that differs from the abstract trait \
             method it implements; only an identical signature is supported",
            owner_label, other.name
        ),
    ))
}

/// Merges imported trait constants with constants declared directly by a class or trait.
///
/// PHP permits duplicate trait constants only when their complete declarations
/// are compatible. Values are folded before comparison, so different syntax
/// producing the same PHP value remains compatible. A differing value,
/// visibility, final flag, type, or attribute is a composition error;
/// compatible local declarations replace the imported copy.
pub(super) fn merge_constants(
    imported: &[ClassConst],
    local: &[ClassConst],
    span: Span,
    owner_label: &str,
) -> Result<Vec<ClassConst>, CompileError> {
    let mut merged = imported.to_vec();
    for constant in local {
        merge_constant_into(
            &mut merged,
            constant.clone(),
            span,
            owner_label,
            true,
        )?;
    }
    // PHP lists the owner's own constants, in declaration order, before the ones its traits
    // bring in (`ReflectionClass::getConstants()` on 8.5.10). The merge above appends a new
    // local after the imported set but replaces a compatible redeclaration IN PLACE, so a
    // redeclared constant would stay ahead of earlier locals. Sorting by each local's own
    // position (imported ones after, stably) restores PHP's order without touching the check.
    merged.sort_by_key(|constant| {
        local
            .iter()
            .position(|own| own.name == constant.name)
            .map_or((1, 0), |index| (0, index))
    });
    Ok(merged)
}

/// Adds constants imported from another trait, rejecting incompatible duplicate declarations.
pub(super) fn merge_imported_constant_set(
    existing: &mut Vec<ClassConst>,
    incoming: Vec<ClassConst>,
    span: Span,
    owner_label: &str,
) -> Result<(), CompileError> {
    for constant in incoming {
        merge_constant_into(existing, constant, span, owner_label, false)?;
    }
    Ok(())
}

/// Inserts one trait constant or validates it against an existing same-named declaration.
fn merge_constant_into(
    merged: &mut Vec<ClassConst>,
    constant: ClassConst,
    span: Span,
    owner_label: &str,
    replace_compatible_existing: bool,
) -> Result<(), CompileError> {
    let Some(index) = merged
        .iter()
        .position(|existing| existing.name == constant.name)
    else {
        merged.push(constant);
        return Ok(());
    };
    if !constants_compatible(&merged[index], &constant) {
        return Err(CompileError::new(
            span,
            &format!(
                "{} has incompatible duplicate trait constant '{}'",
                owner_label, constant.name
            ),
        ));
    }
    if replace_compatible_existing {
        merged[index] = constant;
    }
    Ok(())
}

/// Compares trait constants after folding their values to PHP-equivalent compile-time forms.
fn constants_compatible(left: &ClassConst, right: &ClassConst) -> bool {
    left.name == right.name
        && left.visibility == right.visibility
        && left.is_final == right.is_final
        && left.type_expr == right.type_expr
        && left.attributes == right.attributes
        && crate::optimize::fold_constant_expression(left.value.clone())
            == crate::optimize::fold_constant_expression(right.value.clone())
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
