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
use crate::span::Span;
use crate::types::traits::FlattenedClass;
use crate::types::{PhpType, PropertyHookContract};

use super::super::super::Checker;
use super::super::validation::{
    declared_return_type_compatible, validate_signature_compatibility,
};
use super::state::ClassBuildState;

/// Collects all interfaces (including transitive parents) that `class` implements.
///
/// Validates that each interface exists, that `Throwable` is only implementable by
/// `Error`/`Exception`, and that non-interfaces are not being implemented as interfaces.
/// Pushes collected interface names onto `state.interfaces` in breadth-first order.
pub(super) fn collect_interfaces(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &Checker,
) -> Result<(), CompileError> {
    let mut seen_interfaces: HashSet<String> = state.interfaces.iter().cloned().collect();
    let mut queue = Vec::new();
    for interface_name in class.implements.iter().rev() {
        if interface_is_throwable_contract(checker, interface_name)
            && !class_can_implement_throwable_contract(state, class)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Class {} cannot implement interface Throwable, extend Exception or Error instead",
                    class.name
                ),
            ));
        }
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

/// Returns `true` if `interface_name` is or extends `Throwable` (case-insensitive).
fn interface_is_throwable_contract(checker: &Checker, interface_name: &str) -> bool {
    php_symbol_key(interface_name) == php_symbol_key("Throwable")
        || checker.interface_extends_interface(interface_name, "Throwable")
}

/// Returns `true` if `class` is allowed to implement `Throwable` (must be `Error`, `Exception`,
/// or already implement `Throwable`).
fn class_can_implement_throwable_contract(
    state: &ClassBuildState,
    class: &FlattenedClass,
) -> bool {
    class.name == "Error"
        || class.name == "Exception"
        || state
            .interfaces
            .iter()
            .any(|interface_name| php_symbol_key(interface_name) == php_symbol_key("Throwable"))
}

/// Validates that `class` satisfies all method and property contracts for each interface it
/// implements (including transitive parents).
///
/// For each interface method, calls `validate_interface_method`. For each interface property,
/// calls `validate_interface_property`. Abstract classes are permitted to defer contracts.
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
        for property_name in &interface_info.property_order {
            let contract = interface_info
                .properties
                .get(property_name)
                .expect("type checker bug: missing interface property contract");
            validate_interface_property(
                state,
                class,
                &interface_name,
                property_name,
                contract,
                class_map,
                checker,
                next_class_id,
                building,
            )?;
        }
    }
    Ok(())
}

/// Validates that a concrete (non-abstract) `class` has implementations for all deferred abstract
/// methods and properties accumulated in `state`.
///
/// Returns an error if any instance method sig lacks an implementation class, any static method sig
/// lacks an implementation class, or any abstract property remains undeclared.
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
    if let Some(prop_name) = state.abstract_properties.iter().next() {
        let declaring_class = state
            .property_declaring_classes
            .get(prop_name)
            .cloned()
            .unwrap_or_else(|| class.name.clone());
        let span = state
            .abstract_property_hooks
            .get(prop_name)
            .map(|contract| contract.span)
            .unwrap_or_else(Span::dummy);
        return Err(CompileError::new(
            span,
            &format!(
                "Concrete class {} must declare abstract property {}::${}",
                class.name, declaring_class, prop_name
            ),
        ));
    }
    Ok(())
}

/// Validates that `class` implements the interface method `method_name` from `interface_name`.
///
/// Checks signature compatibility, return type declarations, visibility (must be public), and
/// that non-public static methods cannot satisfy interface contracts. For abstract classes,
/// missing methods are inserted into the class state as deferred contracts.
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

/// Validates that `class` implements the interface property `property_name` from `interface_name`
/// per the `PropertyHookContract`.
///
/// Checks that the property is not static, is publicly visible, and that its type is compatible
/// with the get/set hook contracts. For abstract classes, defers the contract to the class state.
fn validate_interface_property(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    interface_name: &str,
    property_name: &str,
    contract: &PropertyHookContract,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if state
        .static_property_declaring_classes
        .contains_key(property_name)
    {
        return Err(CompileError::new(
            class_property_span(class, property_name, contract.span),
            &format!(
                "Cannot use static property to satisfy interface contract: {}::${}",
                class.name, property_name
            ),
        ));
    }
    if !state.property_declaring_classes.contains_key(property_name) {
        if class.is_abstract {
            defer_interface_property_contract(state, class, interface_name, property_name, contract);
            return Ok(());
        }
        return Err(CompileError::new(
            contract.span,
            &format!(
                "Class {} must implement interface property {}::${}",
                class.name, interface_name, property_name
            ),
        ));
    }

    if state.abstract_properties.contains(property_name) {
        if class.is_abstract {
            state
                .abstract_property_hooks
                .entry(property_name.to_string())
                .or_insert_with(|| contract.clone());
            return Ok(());
        }
        return Ok(());
    }

    if state.property_visibilities.get(property_name) != Some(&Visibility::Public) {
        return Err(CompileError::new(
            class_property_span(class, property_name, contract.span),
            &format!(
                "Interface property implementation must be public: {}::${}",
                class.name, property_name
            ),
        ));
    }

    let actual_ty = instance_property_type_for_contract(state, property_name);
    ensure_object_type_known(&actual_ty, &class.name, class_map, checker, next_class_id, building)?;
    if let Some(required_get) = contract.get_type.as_ref() {
        ensure_object_type_known(
            required_get,
            &class.name,
            class_map,
            checker,
            next_class_id,
            building,
        )?;
        if !checker.type_accepts(required_get, &actual_ty) {
            return Err(CompileError::new(
                class_property_span(class, property_name, contract.span),
                &format!(
                    "Type of {}::${} must be compatible with get property contract {} from interface {}",
                    class.name, property_name, required_get, interface_name
                ),
            ));
        }
    }
    if let Some(required_set) = contract.set_type.as_ref() {
        ensure_object_type_known(
            required_set,
            &class.name,
            class_map,
            checker,
            next_class_id,
            building,
        )?;
        if state.readonly_properties.contains(property_name) {
            return Err(CompileError::new(
                class_property_span(class, property_name, contract.span),
                &format!(
                    "Readonly property {}::${} cannot satisfy set property contract from interface {}",
                    class.name, property_name, interface_name
                ),
            ));
        }
        if !checker.type_accepts(&actual_ty, required_set) {
            return Err(CompileError::new(
                class_property_span(class, property_name, contract.span),
                &format!(
                    "Type of {}::${} must accept set property contract {} from interface {}",
                    class.name, property_name, required_set, interface_name
                ),
            ));
        }
    }
    Ok(())
}

/// Returns the source span of `property_name` in `class`, or `fallback` if not found.
fn class_property_span(
    class: &FlattenedClass,
    property_name: &str,
    fallback: Span,
) -> Span {
    class
        .properties
        .iter()
        .find(|property| property.name == property_name)
        .map(|property| property.span)
        .unwrap_or(fallback)
}

/// Recursively ensures all `PhpType::Object` types referenced in `ty` have been built in `checker`.
///
/// Handles `Object`, `Union`, `Array`, and `AssocArray` recursively; stops for other types.
fn ensure_object_type_known(
    ty: &PhpType,
    current_class: &str,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    match ty {
        PhpType::Object(name) if name != current_class
            && class_map.contains_key(name)
            && !checker.classes.contains_key(name) =>
        {
            super::build_class_info_recursive(name, class_map, checker, next_class_id, building)?;
        }
        PhpType::Union(members) => {
            for member in members {
                ensure_object_type_known(
                    member,
                    current_class,
                    class_map,
                    checker,
                    next_class_id,
                    building,
                )?;
            }
        }
        PhpType::Array(inner) => {
            ensure_object_type_known(
                inner,
                current_class,
                class_map,
                checker,
                next_class_id,
                building,
            )?;
        }
        PhpType::AssocArray { key, value } => {
            ensure_object_type_known(
                key,
                current_class,
                class_map,
                checker,
                next_class_id,
                building,
            )?;
            ensure_object_type_known(
                value,
                current_class,
                class_map,
                checker,
                next_class_id,
                building,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Defers an interface property contract to an abstract class by populating `state` with the
/// property name, type from the contract (or `Mixed`), visibility (public), and abstract marker.
///
/// This allows abstract classes to satisfy interface property requirements without providing an
/// implementation.
fn defer_interface_property_contract(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    interface_name: &str,
    property_name: &str,
    contract: &PropertyHookContract,
) {
    if !state.property_declaring_classes.contains_key(property_name) {
        let slot_index = state.prop_types.len();
        let ty = contract
            .get_type
            .as_ref()
            .or(contract.set_type.as_ref())
            .cloned()
            .unwrap_or(PhpType::Mixed);
        state.prop_types.push((property_name.to_string(), ty));
        state
            .property_offsets
            .insert(property_name.to_string(), 8 + slot_index * 16);
        state.defaults.push(None);
    }
    state
        .property_declaring_classes
        .insert(property_name.to_string(), interface_name.to_string());
    state
        .property_visibilities
        .insert(property_name.to_string(), Visibility::Public);
    state.abstract_properties.insert(property_name.to_string());
    state
        .abstract_property_hooks
        .insert(property_name.to_string(), contract.clone());
    state
        .property_attribute_names
        .entry(property_name.to_string())
        .or_default();
    state
        .property_attribute_args
        .entry(property_name.to_string())
        .or_default();
    state
        .property_declaring_classes
        .entry(property_name.to_string())
        .or_insert_with(|| class.name.clone());
}

/// Returns the runtime `PhpType` for `property_name` from `state`, or `PhpType::Mixed` if the
/// property is not declared.
fn instance_property_type_for_contract(
    state: &ClassBuildState,
    property_name: &str,
) -> PhpType {
    if !state.declared_properties.contains(property_name) {
        return PhpType::Mixed;
    }
    state
        .prop_types
        .iter()
        .find(|(name, _)| name == property_name)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Mixed)
}
