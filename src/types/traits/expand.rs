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
use crate::parser::ast::{
    ClassConst, ClassProperty, TraitAdaptation, TraitUse, Visibility,
};
use crate::span::Span;

use super::{
    final_import_groups, reject_overridden_final_imports, ExpandedTrait, ImportedMethod,
    TraitDeclInfo, TraitMethod,
};
use super::merge::{
    check_abstract_redeclaration, is_same_trait_method, merge_constants,
    merge_imported_constant_set, merge_imported_method_set, merge_methods, merge_properties,
    merge_property_into, trait_method_collision,
};
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
    let (imported_props, imported_methods, imported_constants, bound_methods) = resolve_trait_uses(
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
    reject_overridden_final_imports(
        &final_import_groups(bound_methods),
        &trait_info.methods,
        trait_info.span,
    )?;
    let mut methods = merge_methods(
        imported_methods,
        &trait_info.methods,
        trait_name,
        trait_info.span,
        &format!("trait {}", trait_name),
    )?;
    // Zend builds a trait's function table from its own declarations first and binds the
    // methods of the traits it uses after them, so a class using it lists them in that order
    // (`get_class_methods()`, reflection). `merge_methods` appends the locals last.
    let local_count = trait_info.methods.len();
    let imported_count = methods.len() - local_count;
    methods.rotate_left(imported_count);
    let constants = merge_constants(
        &imported_constants,
        &trait_info.constants,
        trait_info.span,
        &format!("trait {}", trait_name),
    )?;
    let expanded = ExpandedTrait {
        properties,
        methods,
        constants,
    };
    cache.insert(trait_name.to_string(), expanded.clone());
    Ok(expanded)
}

/// For each `TraitUse` in `trait_uses`, expands the referenced traits, applies
/// insteadof/alias adaptations, resolves visibility overrides, selects the
/// dominant method from each `HashMap` of candidates, and accumulates all
/// imported properties, methods, and constants.
///
/// `owner_label` is a human-readable context string (e.g., `"class Foo"` or
/// `"trait Bar"`) used only in error messages. `owner_span` is the source span
/// used for error location. Returns imported properties, methods, and constants, and every
/// method the uses bind, before deduplication, in PHP's binding order (`ImportedMethod::binding`)
/// for the abstract `as final` checks of `reject_overridden_final_imports`.
#[allow(clippy::type_complexity)]
pub(super) fn resolve_trait_uses(
    trait_uses: &[TraitUse],
    trait_map: &HashMap<String, TraitDeclInfo>,
    cache: &mut HashMap<String, ExpandedTrait>,
    stack: &mut Vec<String>,
    owner_label: &str,
    owner_span: Span,
) -> Result<
    (Vec<ClassProperty>, Vec<TraitMethod>, Vec<ClassConst>, Vec<ImportedMethod>),
    CompileError,
> {
    let mut all_properties = Vec::new();
    let mut all_methods: Vec<ImportedMethod> = Vec::new();
    let mut all_constants = Vec::new();
    let mut bound_methods: Vec<ImportedMethod> = Vec::new();
    // PHP lists every used trait of a class-like in `use` order and binds their aliases in
    // declaration order (`ce->traits`, `ce->trait_aliases`).
    let mut trait_position = 0usize;
    let mut alias_position = 0usize;

    for trait_use in trait_uses {
        let mut imported_properties = Vec::new();
        let mut imported_constants = Vec::new();
        let mut candidates: HashMap<String, Vec<ImportedMethod>> = HashMap::new();
        let mut method_order = Vec::new();
        let listed_trait_names: HashSet<String> = trait_use
            .trait_names
            .iter()
            .map(|name| name.as_str().to_string())
            .collect();

        for trait_name in &trait_use.trait_names {
            let expanded =
                expand_trait(trait_name.as_str(), trait_map, cache, stack).map_err(|err| {
                    CompileError::new(
                        trait_use.span,
                        &format!(
                            "{} references unknown or invalid trait '{}': {}",
                            owner_label, trait_name, err.message
                        ),
                    )
                })?;
            let ExpandedTrait {
                properties,
                methods,
                constants,
            } = expanded;
            for property in properties {
                merge_property_into(
                    &mut imported_properties,
                    property,
                    trait_use.span,
                    owner_label,
                    false,
                )?;
            }
            for (method_position, TraitMethod { origin, decl: method }) in
                methods.into_iter().enumerate()
            {
                let method_key = php_symbol_key(&method.name);
                if !candidates.contains_key(&method_key) {
                    method_order.push(method_key.clone());
                }
                candidates
                    .entry(method_key)
                    .or_default()
                    .push(ImportedMethod {
                        source_trait: trait_name.to_string(),
                        origin,
                        decl: method,
                        binding: (trait_position, method_position, usize::MAX),
                    });
            }
            trait_position += 1;
            merge_imported_constant_set(
                &mut imported_constants,
                constants,
                trait_use.span,
                owner_label,
            )?;
        }

        let mut suppressed: HashMap<String, HashSet<String>> = HashMap::new();
        let mut visibility_overrides: HashMap<(String, String), Visibility> = HashMap::new();
        let mut final_overrides: HashSet<(String, String)> = HashSet::new();
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
                    is_final,
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

                    // `foo as final foo` names the method itself: PHP makes it final, as
                    // `foo as final` does, instead of adding a second copy.
                    let same_name_final = *is_final
                        && visibility.is_none()
                        && alias.as_ref().is_some_and(|alias| alias.eq_ignore_ascii_case(method));
                    if let Some(alias_name) = alias.as_ref().filter(|_| !same_name_final) {
                        let mut alias_decl = imported.decl.clone();
                        alias_decl.name = alias_name.clone();
                        if let Some(visibility) = visibility {
                            alias_decl.visibility = visibility.clone();
                        }
                        // `foo as final bar` makes only the alias final (PHP 8.3+).
                        alias_decl.is_final |= *is_final;
                        // An alias binds just before the method it copies.
                        alias_methods.push(ImportedMethod {
                            source_trait: selected_trait,
                            origin: imported.origin.clone(),
                            decl: alias_decl,
                            binding: (imported.binding.0, imported.binding.1, alias_position),
                        });
                        alias_position += 1;
                    } else {
                        let key = (selected_trait, php_symbol_key(method));
                        if *is_final {
                            final_overrides.insert(key.clone());
                        }
                        if let Some(visibility) = visibility {
                            visibility_overrides.insert(key, visibility.clone());
                        }
                    }
                }
            }
        }

        let (selected_methods, bound) = select_methods(
            candidates,
            method_order,
            suppressed,
            visibility_overrides,
            final_overrides,
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
        bound_methods.extend(bound);
        merge_imported_method_set(&mut all_methods, selected_methods, owner_span, owner_label)?;
        merge_imported_constant_set(
            &mut all_constants,
            imported_constants,
            owner_span,
            owner_label,
        )?;
    }

    let all_methods = all_methods
        .into_iter()
        .map(|imported| TraitMethod { origin: imported.origin, decl: imported.decl })
        .collect();
    bound_methods.sort_by_key(|method| method.binding);
    Ok((all_properties, all_methods, all_constants, bound_methods))
}

/// Filters `candidates` using `suppressed` trait-of-origin, applies visibility
/// overrides from `visibility_overrides` and the `as final` adaptations in
/// `final_overrides`, appends aliased methods from `alias_methods`, and returns the
/// final selected method list, with every method bound before deduplication (the kept
/// candidates and the aliases) for the abstract `as final` checks.
///
/// Two concrete candidates for the same method key are a fatal collision (the caller must use
/// insteadof to choose), unless they are the same trait method reached along two `use` paths
/// (`is_same_trait_method`): PHP keeps that one silently.
#[allow(clippy::type_complexity)]
fn select_methods(
    mut candidates: HashMap<String, Vec<ImportedMethod>>,
    method_order: Vec<String>,
    suppressed: HashMap<String, HashSet<String>>,
    visibility_overrides: HashMap<(String, String), Visibility>,
    final_overrides: HashSet<(String, String)>,
    alias_methods: Vec<ImportedMethod>,
    span: Span,
    owner_label: &str,
) -> Result<(Vec<ImportedMethod>, Vec<ImportedMethod>), CompileError> {
    let mut selected_methods = Vec::new();
    let mut bound = Vec::new();
    for method_name in method_order {
        let imports = candidates.remove(&method_name).unwrap_or_default();
        let imports: Vec<ImportedMethod> = imports
            .into_iter()
            // An abstract method made `as final`, here or inside the used trait, stays a
            // requirement after `insteadof`, so the method kept in its place overrides it.
            .filter(|candidate| {
                !suppressed
                    .get(&method_name)
                    .is_some_and(|set| set.contains(&candidate.source_trait))
                    || (candidate.decl.is_abstract
                        && (candidate.decl.is_final
                            || final_overrides.contains(&(
                                candidate.source_trait.clone(),
                                method_name.clone(),
                            ))))
            })
            .map(|mut candidate| {
                // Zend applies `as <visibility>` to the trait's copy before it compares two
                // copies, so `use A, B { A::foo as protected; }` collides even when A and B
                // share the method.
                if let Some(visibility) = visibility_overrides
                    .get(&(candidate.source_trait.clone(), method_name.clone()))
                {
                    candidate.decl.visibility = visibility.clone();
                }
                if final_overrides
                    .contains(&(candidate.source_trait.clone(), method_name.clone()))
                {
                    candidate.decl.is_final = true;
                }
                candidate
            })
            .collect();
        bound.extend(imports.iter().cloned());
        let (requirements, remaining): (Vec<ImportedMethod>, Vec<ImportedMethod>) =
            imports.into_iter().partition(|candidate| candidate.decl.is_abstract);
        let mut concrete: Vec<ImportedMethod> = Vec::new();
        for candidate in remaining {
            if !concrete
                .iter()
                .any(|kept| is_same_trait_method(
                    (&kept.origin, &kept.decl),
                    (&candidate.origin, &candidate.decl),
                ))
            {
                concrete.push(candidate);
            }
        }
        // Two concrete methods collide before any requirement is compared with them.
        if let [applied, rejected, ..] = concrete.as_slice() {
            return Err(trait_method_collision(span, owner_label, applied, rejected));
        }
        let requirements = unsatisfied_requirements(requirements, &concrete, span, owner_label)?;
        selected_methods.extend(concrete.into_iter().chain(requirements));
    }

    bound.extend(alias_methods.iter().cloned());
    selected_methods.extend(alias_methods);

    Ok((selected_methods, bound))
}

/// Keeps the abstract candidates that are still requirements once `concrete` is selected.
///
/// An abstract trait method is a requirement, not a definition, so it is never one of the
/// candidates `insteadof` has to choose between: a concrete candidate of the same kind (static or
/// instance) satisfies it, and several abstract ones of one kind are one requirement once
/// `check_abstract_redeclaration` accepts them. A requirement of the other kind is kept, so the
/// class schema reports the static/instance mismatch as it does for separate `use` statements.
/// A requirement made `as final` that something else overrides is reported separately, from
/// the bound methods (`reject_overridden_final_imports`).
fn unsatisfied_requirements(
    requirements: Vec<ImportedMethod>,
    concrete: &[ImportedMethod],
    span: Span,
    owner_label: &str,
) -> Result<Vec<ImportedMethod>, CompileError> {
    let mut kept: Vec<ImportedMethod> = Vec::new();
    for requirement in requirements {
        let same_kind = concrete
            .iter()
            .chain(kept.iter())
            .find(|other| other.decl.is_static == requirement.decl.is_static);
        match same_kind {
            // A concrete implementation is checked once the class schema is known.
            Some(other) if other.decl.is_abstract => {
                check_abstract_redeclaration(&other.decl, &requirement.decl, span, owner_label)?
            }
            Some(_) => {}
            None => kept.push(requirement),
        }
    }
    Ok(kept)
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
