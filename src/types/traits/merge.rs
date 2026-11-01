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
use crate::parser::ast::{ClassConst, ClassMethod, ClassProperty};
use crate::span::Span;

use super::validation::validate_direct_method_duplicates;

/// Merges a class's own properties with the ones its traits import, in php's order: the class's
/// own declarations first, then each imported property it does not declare itself.
///
/// php compiles the class body before binding its traits, so a trait's properties take the slots
/// after the class's own -- `var_dump()`, `json_encode()`, an `(array)` cast and
/// `ReflectionClass::getProperties()` all list them last. Conflicts are validated exactly as
/// `merge_properties` does; only the order differs.
pub(super) fn merge_class_properties(
    imported: &[ClassProperty],
    local: &[ClassProperty],
    span: Span,
    owner_label: &str,
) -> Result<Vec<ClassProperty>, CompileError> {
    let mut merged = merge_properties(imported, local, span, owner_label, true)?;
    let local_names: HashSet<&str> = local.iter().map(|property| property.name.as_str()).collect();
    let mut ordered = Vec::with_capacity(merged.len());
    for property in local {
        if let Some(index) = merged.iter().position(|existing| existing.name == property.name) {
            ordered.push(merged.remove(index));
        }
    }
    ordered.extend(
        merged
            .into_iter()
            .filter(|property| !local_names.contains(property.name.as_str())),
    );
    Ok(ordered)
}

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

    let mut merged: Vec<ClassMethod> = Vec::new();
    let mut seen_imported: std::collections::HashMap<(String, bool), usize> =
        std::collections::HashMap::new();
    for imported_method in imported {
        let key = (php_symbol_key(&imported_method.name), imported_method.is_static);
        if local_keys.contains(&key) {
            continue;
        }
        if let Some(&index) = seen_imported.get(&key) {
            // Two traits may both require a method (`abstract public function getBytes()`), or
            // one may require what the other implements; php keeps the implementation. Only two
            // implementations collide.
            if imported_method.is_abstract {
                continue;
            }
            if merged[index].is_abstract {
                merged[index] = imported_method;
                continue;
            }
            return Err(CompileError::new(
                span,
                &format!(
                    "{} imports duplicate trait method '{}'",
                    owner_label, imported_method.name
                ),
            ));
        }
        seen_imported.insert(key, merged.len());
        merged.push(imported_method);
    }

    merged.extend(local.iter().cloned());
    Ok(merged)
}

/// Merges a class's (or enum's) trait methods with its own, in the order php's method table
/// holds them: the class's own declarations first, then what its traits bring in.
///
/// `getMethods()` and `get_class_methods()` both read that order, and so does anything a
/// framework derives from them (Symfony's attribute route loader meets controller methods in
/// exactly this sequence). Nested trait flattening keeps `merge_methods`' order.
pub(super) fn merge_class_methods(
    imported: Vec<ClassMethod>,
    local: &[ClassMethod],
    span: Span,
    owner_label: &str,
) -> Result<Vec<ClassMethod>, CompileError> {
    let mut merged = merge_methods(imported, local, span, owner_label)?;
    let imported_count = merged.len() - local.len();
    let imported: Vec<ClassMethod> = merged.drain(..imported_count).collect();
    merged.extend(imported);
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
    for method in incoming {
        let key = (php_symbol_key(&method.name), method.is_static);
        let present = existing
            .iter()
            .position(|other| (php_symbol_key(&other.name), other.is_static) == key);
        match present {
            None => existing.push(method),
            // Abstract requirements from several traits coexist, and an implementation
            // satisfies one (php keeps the implementation); two implementations collide.
            Some(_) if method.is_abstract => {}
            Some(index) if existing[index].is_abstract => existing[index] = method,
            Some(_) => {
                return Err(CompileError::new(
                    span,
                    &format!("{} imports duplicate trait method '{}'", owner_label, method.name),
                ));
            }
        }
    }
    Ok(())
}

/// Adds constants imported from another trait, rejecting incompatible duplicate declarations.
pub(super) fn merge_imported_constant_set(
    existing: &mut Vec<ClassConst>,
    incoming: Vec<ClassConst>,
    span: Span,
    owner_label: &str,
) -> Result<(), CompileError> {
    for constant in incoming {
        merge_constant_into(existing, constant, span, owner_label, false, "trait constant")?;
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

/// Merges imported trait constants with constants declared directly by the consuming class-like.
pub(super) fn merge_constants(
    imported: &[ClassConst],
    local: &[ClassConst],
    span: Span,
    owner_label: &str,
    replace_compatible_existing: bool,
) -> Result<Vec<ClassConst>, CompileError> {
    // php lists a class-like's OWN constants first, in declaration order, and then the ones its
    // traits add (`zend_do_bind_traits` runs after the class body is compiled), so
    // `getConstants()` on `class C { use T; const Z = 1; }` starts with `Z`. A constant the
    // class-like redeclares keeps the class's position; the trait's copy is only validated.
    let mut local_names = HashSet::new();
    for constant in local {
        if !local_names.insert(constant.name.as_str()) {
            return Err(CompileError::new(
                constant.span,
                &format!(
                    "{} declares duplicate constant '{}'",
                    owner_label, constant.name
                ),
            ));
        }
    }
    let mut merged: Vec<ClassConst> = Vec::with_capacity(imported.len() + local.len());
    for constant in imported {
        if let Some(own) = local.iter().find(|own| own.name == constant.name) {
            if !constants_compatible(constant, own) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{} has incompatible duplicate constant '{}'",
                        owner_label, constant.name
                    ),
                ));
            }
            continue;
        }
        merged.push(constant.clone());
    }
    let _ = replace_compatible_existing;
    let mut ordered = local.to_vec();
    ordered.extend(merged);
    Ok(ordered)
}

/// Inserts one constant or validates it against an existing same-named declaration.
///
/// `replace_compatible_existing` lets a class's own declaration take the place of a compatible
/// imported one. The diagnostic names a trait-to-trait clash a "trait constant" and a class
/// declaration clashing with an imported one a plain "constant", which is how each reads.
fn merge_constant_into(
    merged: &mut Vec<ClassConst>,
    constant: ClassConst,
    span: Span,
    owner_label: &str,
    replace_compatible_existing: bool,
    noun: &str,
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
                "{} has incompatible duplicate {} '{}'",
                owner_label, noun, constant.name
            ),
        ));
    }
    if replace_compatible_existing {
        merged[index] = constant;
    }
    Ok(())
}
