//! Purpose:
//! Implements trait validation logic for flattened class metadata.
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
use crate::parser::ast::{ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, TraitUse};
use crate::span::Span;

use super::ExpandedTrait;

/// Rejects constants sharing an enum case name before merging canonical class metadata.
pub(super) fn validate_enum_constant_case_collisions(
    enum_name: &str,
    cases: &[EnumCaseDecl],
    own_constants: &[ClassConst],
    trait_uses: &[TraitUse],
    expanded_traits: &HashMap<String, ExpandedTrait>,
) -> Result<(), CompileError> {
    let case_names: HashSet<&str> = cases.iter().map(|case| case.name.as_str()).collect();
    for constant in own_constants {
        if case_names.contains(constant.name.as_str()) {
            return Err(CompileError::new(
                constant.span,
                &format!("Cannot redefine class constant {}::{}", enum_name, constant.name),
            ));
        }
    }
    for trait_use in trait_uses {
        for trait_name in &trait_use.trait_names {
            let expanded = expanded_traits.get(trait_name.as_str())
                .expect("successfully resolved enum traits are cached");
            for constant in &expanded.constants {
                if case_names.contains(constant.name.as_str()) {
                    return Err(CompileError::new(
                        trait_use.span,
                        &format!(
                            "Cannot use trait {}, because {}::{} conflicts with enum case {}::{}",
                            trait_name, trait_name, constant.name, enum_name, constant.name,
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Validates that `properties` and `methods` within one class or trait body
/// have no duplicate names (case-sensitive). Used to catch direct member conflicts
/// before trait expansion. Returns `Ok(())` or a fatal `CompileError`.
pub(super) fn validate_direct_members(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    span: Span,
    owner_name: &str,
) -> Result<(), CompileError> {
    let mut seen_props = HashSet::new();
    for property in properties {
        if !seen_props.insert(property.name.clone()) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate property declaration in {}: {}", owner_name, property.name),
            ));
        }
    }
    validate_direct_method_duplicates(methods, span, owner_name)
}

/// Validates that `methods` within one class or trait body have no duplicate
/// (name, is_static) keys. Used during merge to catch direct method conflicts
/// before they can be hidden by trait aliasing or insteadof adaptation.
/// Returns `Ok(())` or a fatal `CompileError`.
pub(super) fn validate_direct_method_duplicates(
    methods: &[ClassMethod],
    span: Span,
    owner_name: &str,
) -> Result<(), CompileError> {
    let mut seen = HashSet::new();
    for method in methods {
        let key = (php_symbol_key(&method.name), method.is_static);
        if !seen.insert(key) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate method declaration in {}: {}", owner_name, method.name),
            ));
        }
    }
    Ok(())
}
