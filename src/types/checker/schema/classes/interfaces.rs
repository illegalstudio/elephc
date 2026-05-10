//! Purpose:
//! Validates class schema interfaces rules.
//! Owns one slice of class metadata construction used by object inference and method checking.
//!
//! Called from:
//! - `crate::types::checker::schema::classes`
//!
//! Key details:
//! - Class metadata is shared globally after construction, so validation must reject inconsistent inheritance early.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::super::super::Checker;
use super::super::validation::{
    declared_return_type_compatible, validate_signature_compatibility,
};
use super::state::ClassBuildState;

pub(super) fn collect_interfaces(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &Checker,
) -> Result<(), CompileError> {
    let mut seen_interfaces: HashSet<String> = state.interfaces.iter().cloned().collect();
    let mut queue = Vec::new();
    for interface_name in class.implements.iter().rev() {
        if class_map.contains_key(interface_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Class {} cannot implement non-interface {}",
                    class.name, interface_name
                ),
            ));
        }
        if !checker.interfaces.contains_key(interface_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            ));
        }
        queue.push(interface_name.clone());
    }
    while let Some(interface_name) = queue.pop() {
        if !seen_interfaces.insert(interface_name.clone()) {
            continue;
        }
        let interface_info = checker.interfaces.get(&interface_name).ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            )
        })?;
        for parent_name in interface_info.parents.iter().rev() {
            queue.push(parent_name.clone());
        }
        state.interfaces.push(interface_name);
    }
    Ok(())
}

pub(super) fn validate_interface_contracts(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    for interface_name in state.interfaces.clone() {
        let interface_info = checker.interfaces.get(&interface_name).cloned().ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            )
        })?;
        for method_name in &interface_info.method_order {
            validate_interface_method(
                state,
                class,
                &interface_name,
                method_name,
                class_map,
                checker,
                next_class_id,
                building,
            )?;
        }
    }
    Ok(())
}

pub(super) fn ensure_concrete_class_implements_abstracts(
    state: &ClassBuildState,
    class: &FlattenedClass,
) -> Result<(), CompileError> {
    if class.is_abstract {
        return Ok(());
    }
    if let Some(method_name) = state
        .method_sigs
        .keys()
        .find(|name| !state.method_impl_classes.contains_key(*name))
    {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Concrete class {} must implement abstract method {}::{}",
                class.name, class.name, method_name
            ),
        ));
    }
    if let Some(method_name) = state
        .static_sigs
        .keys()
        .find(|name| !state.static_method_impl_classes.contains_key(*name))
    {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Concrete class {} must implement abstract static method {}::{}",
                class.name, class.name, method_name
            ),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_interface_method(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    interface_name: &str,
    method_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if state.static_sigs.contains_key(method_name) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Cannot use static method to satisfy interface contract: {}::{}",
                class.name, method_name
            ),
        ));
    }
    let interface_info = checker
        .interfaces
        .get(interface_name)
        .expect("type checker bug: interface exists")
        .clone();
    let required_sig = interface_info
        .methods
        .get(method_name)
        .expect("type checker bug: missing interface method signature");
    let actual_sig = match state.method_sigs.get(method_name) {
        Some(sig) => sig,
        None if class.is_abstract => {
            state
                .method_sigs
                .insert(method_name.to_string(), required_sig.clone());
            state
                .method_visibilities
                .insert(method_name.to_string(), Visibility::Public);
            state
                .method_declaring_classes
                .insert(method_name.to_string(), class.name.clone());
            state.method_impl_classes.remove(method_name);
            if !state.vtable_slots.contains_key(method_name) {
                let slot = state.vtable_methods.len();
                state.vtable_slots.insert(method_name.to_string(), slot);
                state.vtable_methods.push(method_name.to_string());
            }
            return Ok(());
        }
        None => {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Class {} must implement interface method {}::{}",
                    class.name, interface_name, method_name
                ),
            ))
        }
    };
    validate_signature_compatibility(
        crate::span::Span::dummy(),
        &class.name,
        method_name,
        actual_sig,
        required_sig,
        "method",
        "implementing interface",
    )?;
    let actual_method = class
        .methods
        .iter()
        .find(|m| php_symbol_key(&m.name) == method_name);
    if required_sig.declared_return && !actual_sig.declared_return {
        return Err(CompileError::new(
            actual_method
                .map(|m| m.span)
                .unwrap_or_else(crate::span::Span::dummy),
            &format!(
                "Cannot implement interface method {}::{} without declaring a compatible return type (interface returns {})",
                class.name, method_name, required_sig.return_type
            ),
        ));
    }
    if let PhpType::Object(actual_name) = &actual_sig.return_type {
        if actual_name != &class.name
            && class_map.contains_key(actual_name)
            && !checker.classes.contains_key(actual_name)
        {
            super::build_class_info_recursive(
                actual_name,
                class_map,
                checker,
                next_class_id,
                building,
            )?;
        }
    }
    if required_sig.declared_return
        && !declared_return_type_compatible(
            checker,
            &required_sig.return_type,
            &actual_sig.return_type,
        )
    {
        return Err(CompileError::new(
            actual_method
                .map(|m| m.span)
                .unwrap_or_else(crate::span::Span::dummy),
            &format!(
                "Cannot implement interface method {}::{} with incompatible return type {} (interface returns {})",
                class.name, method_name, actual_sig.return_type, required_sig.return_type
            ),
        ));
    }
    if state.method_visibilities.get(method_name) != Some(&Visibility::Public) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Interface method implementation must be public: {}::{}",
                class.name, method_name
            ),
        ));
    }
    Ok(())
}
