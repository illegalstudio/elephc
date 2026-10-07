//! Purpose:
//! Validates explicit interface contracts against completed enum class metadata.
//!
//! Called from:
//! - `crate::types::checker::driver` after enum schemas are built.
//!
//! Key details:
//! - Reuses class interface checks without treating enum methods as inherited.
//! - Parameter contravariance and by-reference returns remain PHP contracts.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::types::traits::FlattenedClass;

use super::{interfaces, state::ClassBuildState, Checker};

/// Publishes every transitive enum interface before any cross-enum type compatibility check.
pub(crate) fn expand_enum_interfaces(
    checker: &mut Checker, enum_unit: &FlattenedClass,
) -> Result<(), CompileError> {
    let info = checker.classes.get(&enum_unit.name).expect("enum metadata exists");
    let mut state = ClassBuildState::from_parent(Some(info));
    let implicit = state.interfaces.clone();
    // Direct names are already registered, but marking them seen would skip their parents.
    state.interfaces.clear();
    interfaces::collect_interfaces(&mut state, enum_unit, &HashMap::new(), checker)?;
    for name in implicit {
        if !state.interfaces.contains(&name) { state.interfaces.push(name); }
    }
    checker.classes.get_mut(&enum_unit.name).expect("enum metadata exists").interfaces = state.interfaces;
    Ok(())
}

/// Applies ordinary interface contract checks to the completed, final enum class metadata.
pub(crate) fn validate_enum_interface_contracts(
    checker: &mut Checker, enum_unit: &FlattenedClass,
) -> Result<(), CompileError> {
    if enum_unit.implements.is_empty() { return Ok(()); }
    let info = checker.classes.get(&enum_unit.name).expect("enum metadata exists").clone();
    let mut state = ClassBuildState::from_parent(Some(&info));
    // The enum is the implementation, not a subclass. Include its private and final methods
    // so the shared validator checks their visibility instead of dropping them as inherited.
    state.method_sigs = info.methods.clone();
    state.static_sigs = info.static_methods.clone();
    state.method_visibilities = info.method_visibilities.clone();
    state.static_method_visibilities = info.static_method_visibilities.clone();
    state.method_declaring_classes = info.method_declaring_classes.clone();
    state.static_method_declaring_classes = info.static_method_declaring_classes.clone();
    state.method_impl_classes = info.method_impl_classes.clone();
    state.static_method_impl_classes = info.static_method_impl_classes.clone();
    state.late_static_method_returns = info.late_static_method_returns.clone();
    state.late_static_static_method_returns = info.late_static_static_method_returns.clone();
    let class_map = HashMap::new();
    let mut next_fn_id = 0;
    let mut building = HashSet::new();
    interfaces::validate_interface_contracts(
        &mut state, enum_unit, &class_map, checker, &mut next_fn_id, &mut building,
    )?;
    for name in &state.interfaces {
        let required = checker.interfaces.get(name).expect("validated interface");
        for (methods, actual) in [
            (&required.methods, &state.method_sigs),
            (&required.static_methods, &state.static_sigs),
        ] {
            for (name, contract) in methods {
                let implementation = actual.get(name).expect("validated implementation");
                for (index, ((parameter, actual_ty), (_, required_ty))) in implementation.params
                    .iter().zip(&contract.params).enumerate()
                {
                    let actual_ty = if implementation.declared_params.get(index).copied().unwrap_or(false) {
                        actual_ty
                    } else { &crate::types::PhpType::Mixed };
                    let required_ty = if contract.declared_params.get(index).copied().unwrap_or(false) {
                        required_ty
                    } else { &crate::types::PhpType::Mixed };
                    if !super::super::class_constants::strict_type_accepts(checker, actual_ty, required_ty, false) {
                        return Err(CompileError::new(enum_unit.span, &format!(
                            "Cannot narrow interface parameter ${parameter}: {}::{name}", enum_unit.name,
                        )));
                    }
                }
                if contract.by_ref_return && !implementation.by_ref_return {
                    return Err(CompileError::new(enum_unit.span, &format!(
                        "Cannot remove by-reference return when implementing interface method: {}::{name}",
                        enum_unit.name,
                    )));
                }
            }
        }
    }
    checker.classes.get_mut(&enum_unit.name).expect("enum metadata exists").interfaces = state.interfaces;
    Ok(())
}
