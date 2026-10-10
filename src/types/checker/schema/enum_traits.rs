//! Purpose:
//! Validates abstract trait method requirements against an enum's final implementations.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()` after enum schema construction.
//!
//! Key details:
//! - Local methods win trait composition but must not erase abstract requirements.
//! - Nested trait requirements bind relative types to the consuming enum.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, StmtKind, TraitAdaptation, TraitUse};
use crate::types::{traits::FlattenedClass, PhpType};

use super::super::Checker;
use super::class_constants::strict_type_accepts;
use super::validation::{
    build_method_sig, late_static_return_compatible, validate_abstract_trait_signature, visibility_rank,
};

/// Raw requirements remain available even when trait flattening selects a concrete override.
type TraitMembers<'a> = (&'a [TraitUse], &'a [ClassMethod]);

/// Checks every recursively imported abstract contract against the enum's selected method.
pub(crate) fn validate_enum_trait_requirements(
    checker: &Checker,
    program: &Program,
    enum_unit: &FlattenedClass,
) -> Result<(), CompileError> {
    if enum_unit.used_traits.is_empty() {
        return Ok(());
    }
    let mut traits = HashMap::new();
    collect_trait_members(program, &mut traits);
    if let Some(uses) = enum_trait_uses(program, &enum_unit.name) {
        for required in adapted_requirements(uses, &traits, &mut HashSet::new()) {
            let Some(actual) = enum_unit.methods.iter().find(|method| {
                !method.is_abstract && method.is_static == required.is_static
                    && php_symbol_key(&method.name) == php_symbol_key(&required.name)
            }) else { continue; };
            validate_requirement(checker, enum_unit, &required, actual)?;
        }
    }
    Ok(())
}

/// Finds the consuming enum's source adaptations, including namespace-only checker fixtures.
fn enum_trait_uses<'a>(program: &'a Program, name: &str) -> Option<&'a [TraitUse]> {
    program.iter().find_map(|stmt| match &stmt.kind {
        StmtKind::EnumDecl { name: candidate, trait_uses, .. } if candidate == name => {
            Some(trait_uses.as_slice())
        }
        StmtKind::NamespaceBlock { body, .. } => enum_trait_uses(body, name),
        _ => None,
    })
}

/// Preserves abstract requirements through each nested trait alias and visibility adaptation.
fn adapted_requirements(
    uses: &[TraitUse], traits: &HashMap<String, TraitMembers<'_>>, visiting: &mut HashSet<String>,
) -> Vec<ClassMethod> {
    let mut requirements = Vec::new();
    for usage in uses {
        let mut imported = Vec::new();
        for name in &usage.trait_names {
            let key = php_symbol_key(name.as_str());
            if !visiting.insert(key.clone()) { continue; }
            if let Some((nested, methods)) = traits.get(&key) {
                let mut members = adapted_requirements(nested, traits, visiting);
                members.extend(methods.iter().filter(|method| method.is_abstract).cloned());
                imported.extend(members.into_iter().map(|method| (key.clone(), method)));
            }
            visiting.remove(&key);
        }
        for adaptation in &usage.adaptations {
            match adaptation {
                // Selecting a concrete body does not discard another trait's abstract contract.
                TraitAdaptation::InsteadOf { .. } => {}
                TraitAdaptation::Alias { trait_name, method, alias, visibility, .. } => {
                    let mut aliases = Vec::new();
                    for (origin, required) in &mut imported {
                        if php_symbol_key(&required.name) != php_symbol_key(method)
                            || trait_name.as_ref().is_some_and(|name| php_symbol_key(name.as_str()) != *origin)
                        { continue; }
                        if let Some(alias) = alias {
                            let mut adapted = required.clone();
                            adapted.name = alias.clone();
                            if let Some(visibility) = visibility { adapted.visibility = visibility.clone(); }
                            aliases.push((origin.clone(), adapted));
                        } else if let Some(visibility) = visibility {
                            required.visibility = visibility.clone();
                        }
                    }
                    imported.extend(aliases);
                }
            }
        }
        requirements.extend(imported.into_iter().map(|(_, method)| method));
    }
    requirements
}

/// Collects raw trait declarations, including namespace blocks used by checker-only fixtures.
fn collect_trait_members<'a>(program: &'a Program, traits: &mut HashMap<String, TraitMembers<'a>>) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, trait_uses, methods, .. } => {
                traits.insert(php_symbol_key(name), (trait_uses, methods));
            }
            StmtKind::NamespaceBlock { body, .. } => collect_trait_members(body, traits),
            _ => {}
        }
    }
}

/// Applies shared override rules and non-coercive parameter variance to one abstract requirement.
fn validate_requirement(
    checker: &Checker,
    enum_unit: &FlattenedClass,
    required: &ClassMethod,
    actual: &ClassMethod,
) -> Result<(), CompileError> {
    let mut required = required.clone();
    required.substitute_relative_class_types(&enum_unit.name, None);
    let required_sig = build_method_sig(checker, &required, &enum_unit.name)?;
    let actual_sig = build_method_sig(checker, actual, &enum_unit.name)?;
    validate_abstract_trait_signature(
        checker, actual.span, &enum_unit.name, &actual.name, &actual_sig, &required_sig,
    )?;
    let late_static_compatible = late_static_return_compatible(
        checker, required.return_type.as_ref().filter(|hint| hint.contains_late_static()),
        actual.return_type.as_ref(), &actual_sig.return_type, &enum_unit.name, actual.span,
    )?;
    if visibility_rank(&actual.visibility) < visibility_rank(&required.visibility) {
        return Err(CompileError::new(actual.span, &format!(
            "Cannot reduce visibility when implementing trait method: {}::{}",
            enum_unit.name, actual.name,
        )));
    }
    if required_sig.by_ref_return && !actual_sig.by_ref_return {
        return Err(CompileError::new(actual.span, &format!(
            "Cannot remove by-reference return when implementing trait method: {}::{}",
            enum_unit.name, actual.name,
        )));
    }
    if required_sig.declared_return
        && !matches!(actual_sig.return_type, PhpType::Never)
        && (!actual_sig.declared_return
            || !late_static_compatible.unwrap_or_else(|| {
                strict_type_accepts(checker, &required_sig.return_type, &actual_sig.return_type, false)
            }))
    {
        return Err(CompileError::new(actual.span, &format!(
            "Enum method {}::{} has incompatible return type for abstract trait method",
            enum_unit.name, actual.name,
        )));
    }
    Ok(())
}
