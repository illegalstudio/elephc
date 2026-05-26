//! Purpose:
//! Implements trait expand logic for flattened class metadata.
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
use crate::parser::ast::{ClassMethod, ClassProperty, TraitAdaptation, TraitUse, Visibility};
use crate::span::Span;

use super::{ExpandedTrait, ImportedMethod, TraitDeclInfo};
use super::merge::{merge_imported_method_set, merge_methods, merge_properties, merge_property_into};
use super::validation::validate_direct_members;

/// Recursively expands a single trait, applying its trait_uses, then merging
/// all inherited and direct members. Uses `cache` to memoize results and `stack`
/// to detect circular composition. Returns the fully expanded property/method set
/// or a `CompileError` on circular reference, unknown trait, or validation failure.
fn expand_trait(
    trait_name: &str,
    trait_map: &HashMap<String, TraitDeclInfo>,
    cache: &mut HashMap<String, ExpandedTrait>,
    stack: &mut Vec<String>,
) -> Result<ExpandedTrait, CompileError> {
    if let Some(expanded) = cache.get(trait_name) {
        return Ok(expanded.clone());
    }
    if stack.iter().any(|name| name == trait_name) {
        let mut chain = stack.clone();
        chain.push(trait_name.to_string());
        return Err(CompileError::new(
            Span::dummy(),
            &format!("Circular trait composition detected: {}", chain.join(" -> ")),
        ));
    }
    let trait_info = trait_map.get(trait_name).ok_or_else(|| {
        CompileError::new(
            Span::dummy(),
            &format!("Unknown trait referenced during flattening: {}", trait_name),
        )
    })?;

    validate_direct_members(
        &trait_info.properties,
        &trait_info.methods,
        trait_info.span,
        trait_name,
    )?;

    stack.push(trait_name.to_string());
    let (imported_props, imported_methods) = resolve_trait_uses(
        &trait_info.trait_uses,
        trait_map,
        cache,
        stack,
        &format!("trait {}", trait_name),
        trait_info.span,
    )?;
    stack.pop();

    let properties = merge_properties(
        &imported_props,
        &trait_info.properties,
        trait_info.span,
        &format!("trait {}", trait_name),
        true,
    )?;
    let methods = merge_methods(
        imported_methods,
        &trait_info.methods,
        trait_info.span,
        &format!("trait {}", trait_name),
    )?;
    let expanded = ExpandedTrait { properties, methods };
    cache.insert(trait_name.to_string(), expanded.clone());
    Ok(expanded)
}

/// For each `TraitUse` in `trait_uses`, expands the referenced traits, applies
/// insteadof/alias adaptations, resolves visibility overrides, selects the
/// dominant method from each `HashMap` of candidates, and accumulates all
/// imported properties and methods into `all_properties` and `all_methods`.
///
/// `owner_label` is a human-readable context string (e.g., `"class Foo"` or
/// `"trait Bar"`) used only in error messages. `owner_span` is the source span
/// used for error location. Returns `([ClassProperty], [ClassMethod])` on success.
pub(super) fn resolve_trait_uses(
    trait_uses: &[TraitUse],
    trait_map: &HashMap<String, TraitDeclInfo>,
    cache: &mut HashMap<String, ExpandedTrait>,
    stack: &mut Vec<String>,
    owner_label: &str,
    owner_span: Span,
) -> Result<(Vec<ClassProperty>, Vec<ClassMethod>), CompileError> {
    let mut all_properties = Vec::new();
    let mut all_methods = Vec::new();

    for trait_use in trait_uses {
        let mut imported_properties = Vec::new();
        let mut candidates: HashMap<String, Vec<ImportedMethod>> = HashMap::new();
        let mut method_order = Vec::new();
        let listed_trait_names: HashSet<String> = trait_use
            .trait_names
            .iter()
            .map(|name| name.as_str().to_string())
            .collect();

        for trait_name in &trait_use.trait_names {
            let expanded = expand_trait(trait_name.as_str(), trait_map, cache, stack).map_err(|err| {
                CompileError::new(
                    trait_use.span,
                    &format!("{} references unknown or invalid trait '{}': {}", owner_label, trait_name, err.message),
                )
            })?;
            for property in expanded.properties {
                merge_property_into(
                    &mut imported_properties,
                    property,
                    trait_use.span,
                    owner_label,
                    false,
                )?;
            }
            for method in expanded.methods {
                let method_key = php_symbol_key(&method.name);
                if !candidates.contains_key(&method_key) {
                    method_order.push(method_key.clone());
                }
                candidates
                    .entry(method_key)
                    .or_default()
                    .push(ImportedMethod {
                        source_trait: trait_name.to_string(),
                        decl: method,
                    });
            }
        }

        let mut suppressed: HashMap<String, HashSet<String>> = HashMap::new();
        let mut visibility_overrides: HashMap<(String, String), Visibility> = HashMap::new();
        let mut alias_methods: Vec<ImportedMethod> = Vec::new();

        for adaptation in &trait_use.adaptations {
            match adaptation {
                TraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                } => {
                    let selected_trait = resolve_adaptation_source(
                        trait_name.as_ref().map(|name| name.as_str()),
                        method,
                        &candidates,
                        trait_use.span,
                    )?;
                    for loser in instead_of {
                        if !listed_trait_names.contains(loser.as_str()) {
                            return Err(CompileError::new(
                                trait_use.span,
                                &format!(
                                    "{} cannot use insteadof with unrelated trait '{}'",
                                    owner_label, loser
                                ),
                            ));
                        }
                        if loser.as_str() == selected_trait {
                            return Err(CompileError::new(
                                trait_use.span,
                                &format!(
                                    "{} cannot suppress the same trait '{}' with insteadof",
                                    owner_label, loser
                                ),
                            ));
                        }
                        suppressed
                            .entry(php_symbol_key(method))
                            .or_default()
                            .insert(loser.to_string());
                    }
                }
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    let selected_trait = resolve_adaptation_source(
                        trait_name.as_ref().map(|name| name.as_str()),
                        method,
                        &candidates,
                        trait_use.span,
                    )?;
                    let method_key = php_symbol_key(method);
                    let imported = candidates
                        .get(&method_key)
                        .and_then(|methods| {
                            methods
                                .iter()
                                .find(|candidate| candidate.source_trait == selected_trait)
                        })
                        .cloned()
                        .ok_or_else(|| {
                            CompileError::new(
                                trait_use.span,
                                &format!(
                                    "{} cannot alias undefined trait method {}::{}",
                                    owner_label, selected_trait, method
                                ),
                            )
                        })?;

                    if let Some(alias_name) = alias {
                        let mut alias_decl = imported.decl.clone();
                        alias_decl.name = alias_name.clone();
                        if let Some(visibility) = visibility {
                            alias_decl.visibility = visibility.clone();
                        }
                        alias_methods.push(ImportedMethod {
                            source_trait: selected_trait,
                            decl: alias_decl,
                        });
                    } else if let Some(visibility) = visibility {
                        visibility_overrides
                            .insert((selected_trait, php_symbol_key(method)), visibility.clone());
                    }
                }
            }
        }

        let selected_methods = select_methods(
            candidates,
            method_order,
            suppressed,
            visibility_overrides,
            alias_methods,
            trait_use.span,
            owner_label,
        )?;

        all_properties = merge_properties(
            &all_properties,
            &imported_properties,
            owner_span,
            owner_label,
            false,
        )?;
        merge_imported_method_set(&mut all_methods, selected_methods, owner_span, owner_label)?;
    }

    Ok((all_properties, all_methods))
}

/// Filters `candidates` using `suppressed` trait-of-origin, applies visibility
/// overrides from `visibility_overrides`, appends aliased methods from
/// `alias_methods`, and returns the final selected method list.
///
/// Ambiguity (multiple non-suppressed candidates for the same method key) is
/// a fatal error; the caller must have already emitted insteadof to disambiguate.
fn select_methods(
    mut candidates: HashMap<String, Vec<ImportedMethod>>,
    method_order: Vec<String>,
    suppressed: HashMap<String, HashSet<String>>,
    visibility_overrides: HashMap<(String, String), Visibility>,
    alias_methods: Vec<ImportedMethod>,
    span: Span,
    owner_label: &str,
) -> Result<Vec<ClassMethod>, CompileError> {
    let mut selected_methods = Vec::new();
    for method_name in method_order {
        let imports = candidates.remove(&method_name).unwrap_or_default();
        let remaining: Vec<ImportedMethod> = imports
            .into_iter()
            .filter(|candidate| {
                !suppressed
                    .get(&method_name)
                    .is_some_and(|set| set.contains(&candidate.source_trait))
            })
            .collect();
        if remaining.len() > 1 {
            let trait_list = remaining
                .iter()
                .map(|candidate| candidate.source_trait.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CompileError::new(
                span,
                &format!(
                    "{} has ambiguous trait method '{}'; resolve with insteadof (candidates: {})",
                    owner_label, method_name, trait_list
                ),
            ));
        }
        if let Some(mut selected) = remaining.into_iter().next() {
            if let Some(visibility) = visibility_overrides.get(&(
                selected.source_trait.clone(),
                method_name.clone(),
            )) {
                selected.decl.visibility = visibility.clone();
            }
            selected_methods.push(selected.decl);
        }
    }

    for alias_method in alias_methods {
        selected_methods.push(alias_method.decl);
    }

    Ok(selected_methods)
}

/// Resolves the source trait for a trait adaptation (insteadof or alias).
///
/// If `explicit_trait` is provided, validates that the method exists on that
/// trait and returns it. If not provided and exactly one candidate remains,
/// returns that candidate's trait. Otherwise returns an ambiguity error.
fn resolve_adaptation_source(
    explicit_trait: Option<&str>,
    method: &str,
    candidates: &HashMap<String, Vec<ImportedMethod>>,
    span: Span,
) -> Result<String, CompileError> {
    let method_key = php_symbol_key(method);
    let options = candidates.get(&method_key).ok_or_else(|| {
        CompileError::new(
            span,
            &format!("Trait adaptation references undefined method '{}'", method),
        )
    })?;

    if let Some(trait_name) = explicit_trait {
        if options
            .iter()
            .any(|candidate| candidate.source_trait == trait_name)
        {
            return Ok(trait_name.to_string());
        }
        return Err(CompileError::new(
            span,
            &format!(
                "Trait adaptation references undefined method {}::{}",
                trait_name, method
            ),
        ));
    }

    if options.len() == 1 {
        Ok(options[0].source_trait.clone())
    } else {
        Err(CompileError::new(
            span,
            &format!(
                "Trait adaptation for '{}' is ambiguous without a qualifying trait name",
                method
            ),
        ))
    }
}
