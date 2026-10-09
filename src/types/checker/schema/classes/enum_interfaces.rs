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
use super::super::validation::SourceVisibleShape;

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
    let mut collected_variadics = Vec::new();
    for name in &state.interfaces {
        let required = checker.interfaces.get(name).expect("validated interface");
        for (is_static, methods, actual) in [
            (false, &required.methods, &state.method_sigs),
            (true, &required.static_methods, &state.static_sigs),
        ] {
            for (name, contract) in methods {
                let implementation = actual.get(name).expect("validated implementation");
                let actual = SourceVisibleShape::of(implementation);
                let required = SourceVisibleShape::of(contract);
                for index in 0..actual.param_count.max(required.param_count) {
                    let Some(((actual_ty, _), (required_ty, _))) =
                        actual.parameter_at(index).zip(required.parameter_at(index)) else { continue; };
                    if !super::super::class_constants::strict_type_accepts(checker, actual_ty, required_ty, false) {
                        return Err(CompileError::new(enum_unit.span, &format!(
                            "Cannot narrow interface parameter ${}: {}::{name}",
                            actual.param_names[index.min(actual.param_count - 1)], enum_unit.name,
                        )));
                    }
                }
                if contract.by_ref_return && !implementation.by_ref_return {
                    return Err(CompileError::new(enum_unit.span, &format!(
                        "Cannot remove by-reference return when implementing interface method: {}::{name}",
                        enum_unit.name,
                    )));
                }
                if crate::func_args::sig_collects_surplus_args(contract)
                    && implementation.variadic.is_some()
                    && !crate::func_args::sig_collects_surplus_args(implementation)
                {
                    collected_variadics.push((name.clone(), is_static));
                }
            }
        }
    }
    checker.classes.get_mut(&enum_unit.name).expect("enum metadata exists").interfaces = state.interfaces;
    // Captured interface callers transport surplus arguments in Mixed cells. The
    // shared promotion changes only storage, preserving the declared element hint.
    for (method, is_static) in collected_variadics {
        checker.promote_descriptor_variadic_container_for_method(&enum_unit.name, &method, is_static);
    }
    Ok(())
}
