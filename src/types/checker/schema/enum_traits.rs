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
use crate::parser::ast::{ClassMethod, Program, StmtKind, TraitUse};
use crate::types::{traits::FlattenedClass, PhpType};

use super::super::Checker;
use super::class_constants::strict_type_accepts;
use super::validation::{build_method_sig, validate_override_signature, visibility_rank};

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
    let mut pending = enum_unit.used_traits.clone();
    let mut visited = HashSet::new();
    while let Some(name) = pending.pop() {
        let key = php_symbol_key(&name);
        if !visited.insert(key.clone()) {
            continue;
        }
        let Some((uses, methods)) = traits.get(&key) else { continue; };
        for usage in *uses {
            pending.extend(usage.trait_names.iter().map(|name| name.as_str().to_string()));
        }
        for required in methods.iter().filter(|method| method.is_abstract) {
            let Some(actual) = enum_unit.methods.iter().find(|method| {
                !method.is_abstract && method.is_static == required.is_static
                    && php_symbol_key(&method.name) == php_symbol_key(&required.name)
            }) else { continue; };
            validate_requirement(checker, enum_unit, required, actual)?;
        }
    }
    Ok(())
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
    validate_override_signature(
        checker, enum_unit, actual, &required_sig,
        required.return_type.as_ref().filter(|hint| hint.contains_late_static()),
        actual.is_static, true,
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
    for (index, ((name, actual_ty), (_, required_ty))) in actual_sig.params.iter()
        .zip(&required_sig.params).enumerate()
    {
        let actual_ty = if actual_sig.declared_params.get(index).copied().unwrap_or(false) {
            actual_ty
        } else { &PhpType::Mixed };
        let required_ty = if required_sig.declared_params.get(index).copied().unwrap_or(false) {
            required_ty
        } else { &PhpType::Mixed };
        if !strict_type_accepts(checker, actual_ty, required_ty, false) {
            return Err(CompileError::new(actual.span, &format!(
                "Cannot narrow parameter ${name} when implementing trait method: {}::{}",
                enum_unit.name, actual.name,
            )));
        }
    }
    if required_sig.declared_return
        && !matches!(actual_sig.return_type, PhpType::Never)
        && !strict_type_accepts(checker, &required_sig.return_type, &actual_sig.return_type, false)
    {
        return Err(CompileError::new(actual.span, &format!(
            "Enum method {}::{} has incompatible return type for abstract trait method",
            enum_unit.name, actual.name,
        )));
    }
    Ok(())
}
