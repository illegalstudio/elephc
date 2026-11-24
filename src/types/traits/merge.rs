//! Purpose:
//! Implements trait merge logic for flattened class metadata.
//! Applies PHP trait composition rules before object inference and method checks consume class schemas.
//!
//! Called from:
//! - `crate::types::traits`
//!
//! Key details:
//! - Merge and validation rules must report conflicts early because downstream class metadata is treated as canonical.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassConst, ClassMethod, ClassProperty};
use crate::span::Span;

use super::validation::validate_direct_method_duplicates;
use super::{ImportedMethod, TraitMethod};

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
/// Imported methods with the same key as local are skipped (local wins), except an abstract
/// import made final with `as final`, which the local method cannot override.
/// Duplicate imported methods (same key from multiple traits) are resolved by
/// `resolve_duplicate_imported_method`; `resolve_trait_uses` has already merged them, so a
/// collision here is reported without trait names.
/// Returns the merged list: imported-first, then local, each local method with `local_origin`
/// (the declaring trait, or the class) as its origin.
pub(super) fn merge_methods(
    imported: Vec<TraitMethod>,
    local: &[ClassMethod],
    local_origin: &str,
    span: Span,
    owner_label: &str,
) -> Result<Vec<TraitMethod>, CompileError> {
    validate_direct_method_duplicates(local, span, owner_label)?;

    let mut local_keys = HashMap::new();
    for method in local {
        local_keys.insert((php_symbol_key(&method.name), method.is_static), method.span);
    }

    let mut merged: Vec<TraitMethod> = Vec::new();
    let mut seen_imported: HashMap<(String, bool), usize> = HashMap::new();
    for imported_method in imported {
        let key = (php_symbol_key(&imported_method.decl.name), imported_method.decl.is_static);
        if let Some(local_span) = local_keys.get(&key) {
            // Zend checks a local method against an abstract trait method as an override, so an
            // abstract method made `as final` cannot be implemented here. A concrete trait
            // method stays replaceable, final or not.
            if imported_method.decl.is_abstract && imported_method.decl.is_final {
                return Err(CompileError::new(
                    *local_span,
                    &format!(
                        "Cannot override final method {}::{}",
                        imported_method.origin, imported_method.decl.name
                    ),
                ));
            }
            continue;
        }
        if let Some(&index) = seen_imported.get(&key) {
            match resolve_duplicate_imported_method(
                (&merged[index].origin, &merged[index].decl),
                (&imported_method.origin, &imported_method.decl),
                span,
                owner_label,
            )? {
                DuplicateImport::KeepExisting => {}
                DuplicateImport::ReplaceExisting => merged[index] = imported_method,
                DuplicateImport::Collision => {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "{} imports duplicate trait method '{}'",
                            owner_label, imported_method.decl.name
                        ),
                    ));
                }
            }
            continue;
        }
        seen_imported.insert(key, merged.len());
        merged.push(imported_method);
    }

    merged.extend(local.iter().map(|method| TraitMethod {
        origin: local_origin.to_string(),
        decl: method.clone(),
    }));
    Ok(merged)
}

/// Appends each method in `incoming` to `existing`, tracking seen (name, is_static)
/// keys to prevent duplicate imports across different trait sources.
/// A second method with a key already in `existing` is resolved by
/// `resolve_duplicate_imported_method`; a collision reports both source traits as PHP does.
/// Used when accumulating methods from multiple traits within one class.
pub(super) fn merge_imported_method_set(
    existing: &mut Vec<ImportedMethod>,
    incoming: Vec<ImportedMethod>,
    span: Span,
    owner_label: &str,
) -> Result<(), CompileError> {
    let mut seen: HashMap<(String, bool), usize> = existing
        .iter()
        .enumerate()
        .map(|(index, method)| ((php_symbol_key(&method.decl.name), method.decl.is_static), index))
        .collect();
    for method in incoming {
        let key = (php_symbol_key(&method.decl.name), method.decl.is_static);
        if let Some(&index) = seen.get(&key) {
            match resolve_duplicate_imported_method(
                (&existing[index].origin, &existing[index].decl),
                (&method.origin, &method.decl),
                span,
                owner_label,
            )? {
                DuplicateImport::KeepExisting => {}
                DuplicateImport::ReplaceExisting => existing[index] = method,
                DuplicateImport::Collision => {
                    return Err(trait_method_collision(span, owner_label, &existing[index], &method));
                }
            }
            continue;
        }
        seen.insert(key, existing.len());
        existing.push(method);
    }
    Ok(())
}

/// What to do with a second imported method that has the key of an existing one.
enum DuplicateImport {
    KeepExisting,
    ReplaceExisting,
    Collision,
}

/// Resolves a second imported method with the key of `existing`, as PHP does: an abstract trait
/// method is a requirement, not a definition, so it never conflicts. A concrete method replaces
/// an abstract one (`use NeedsInvalidate, ProvidesInvalidate;`), and a second abstract one adds
/// nothing once `check_abstract_redeclaration` accepts the pair. The same trait method reached
/// along two `use` paths is kept once (`is_same_trait_method`). Only two different concrete
/// methods collide.
fn resolve_duplicate_imported_method(
    existing: (&str, &ClassMethod),
    incoming: (&str, &ClassMethod),
    span: Span,
    owner_label: &str,
) -> Result<DuplicateImport, CompileError> {
    if incoming.1.is_abstract {
        if existing.1.is_abstract {
            check_abstract_redeclaration(existing.1, incoming.1, span, owner_label)?;
        }
        return Ok(DuplicateImport::KeepExisting);
    }
    if existing.1.is_abstract {
        return Ok(DuplicateImport::ReplaceExisting);
    }
    if is_same_trait_method(existing, incoming) {
        return Ok(DuplicateImport::KeepExisting);
    }
    Ok(DuplicateImport::Collision)
}

/// Checks that `other`, a second abstract declaration of the abstract trait method `required`
/// from another trait, matches it.
///
/// PHP checks that pair like an override (`Declaration of A::foo(): int must be compatible with
/// B::foo(): string`). Until flattening runs that check, only an identical signature is
/// accepted: the same parameter types, by-reference flags, optionality and variadic, and the
/// same return type and by-reference return. Visibility is not compared, as PHP does not. A
/// CONCRETE method that implements a requirement is not checked here:
/// `validate_class_trait_requirements` and `validate_enum_trait_requirements` run PHP's
/// compatibility rules on it once the class schema is known.
pub(super) fn check_abstract_redeclaration(
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
            "{} imports abstract trait method '{}' twice with different signatures; only an \
             identical signature is supported",
            owner_label, other.name
        ),
    ))
}

/// Whether two imported copies, each with its declaring trait, are the same trait method
/// reached along two `use` paths (`trait A { use T; } trait B { use T; } class C { use A, B; }`).
///
/// Zend keeps the first copy when both share the declaration's opcodes and their visibility
/// matches, and reports a collision otherwise, even for identical source in two traits. The
/// declaring trait and the declaration span identify the source method: an `as` alias keeps
/// both, and a generic trait's instantiations share spans but are distinct traits, so their
/// methods differ in origin even when only a body mentions the type parameter. Flattening
/// changes nothing else in a copy but its visibility (`as <visibility>`), compared here too.
pub(super) fn is_same_trait_method(
    (left_origin, left): (&str, &ClassMethod),
    (right_origin, right): (&str, &ClassMethod),
) -> bool {
    left.span == right.span
        && left_origin.eq_ignore_ascii_case(right_origin)
        && left.visibility == right.visibility
        && left.is_static == right.is_static
        && php_symbol_key(&left.name) == php_symbol_key(&right.name)
}

/// PHP's error for two different trait methods with one name: `rejected` is the later one,
/// which is not applied.
pub(super) fn trait_method_collision(
    span: Span,
    owner_label: &str,
    applied: &ImportedMethod,
    rejected: &ImportedMethod,
) -> CompileError {
    let owner = owner_label
        .split_once(' ')
        .map_or(owner_label, |(_, name)| name);
    CompileError::new(
        span,
        &format!(
            "Trait method {}::{} has not been applied as {}::{}, because of collision with {}::{}",
            rejected.source_trait,
            rejected.decl.name,
            owner,
            rejected.decl.name,
            applied.source_trait,
            applied.decl.name,
        ),
    )
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
