//! Purpose:
//! Validates schema interfaces declarations for the checker.
//! Turns parsed declarations into canonical metadata and rejects invalid contracts before code generation.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Declaration metadata must align with name resolution, inheritance flattening, and runtime/codegen expectations.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassProperty, Visibility};
use crate::types::{InterfaceInfo, PhpType, PropertyHookContract};

use super::super::Checker;
use super::super::InterfaceDeclInfo;
use super::validation::{build_method_sig, validate_signature_compatibility};
use crate::types::traits::FlattenedClass;

/// Recursively builds interface metadata by flattening inheritance and collecting methods,
/// properties, and constants. Detects circular inheritance, validates signature compatibility
/// across parents, and stores the final `InterfaceInfo` in the checker's interface map.
///
/// - `interface_name`: Name of the interface to build.
/// - `interface_map`: Raw parsed interface declarations from the schema pass.
/// - `class_map`: Flattened class map for validating that interfaces do not extend classes.
/// - `checker`: Mutable checker state; result is written to `checker.interfaces`.
/// - `next_interface_id`: monotonically increasing ID assigned to each processed interface.
/// - `building`: Tracks interfaces currently being processed to detect cycles.
///
/// Returns `Ok(())` on success or a `CompileError` for circular inheritance, unknown parents,
/// duplicate declarations, or incompatible method signatures across parent interfaces.
pub(crate) fn build_interface_info_recursive(
    interface_name: &str,
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_interface_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if checker.interfaces.contains_key(interface_name) {
        return Ok(());
    }

    if !building.insert(interface_name.to_string()) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Circular interface inheritance detected involving {}",
                interface_name
            ),
        ));
    }

    let interface = interface_map.get(interface_name).cloned().ok_or_else(|| {
        CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Unknown interface referenced during interface flattening: {}",
                interface_name
            ),
        )
    })?;

    let mut methods = HashMap::new();
    let mut method_declaring_interfaces = HashMap::new();
    let mut method_order = Vec::new();
    let mut method_slots = HashMap::new();
    let mut properties = HashMap::new();
    let mut property_order = Vec::new();

    for parent_name in &interface.extends {
        if class_map.contains_key(parent_name) {
            return Err(CompileError::new(
                interface.span,
                &format!(
                    "Interface {} cannot extend class {}; only interfaces are allowed",
                    interface.name, parent_name
                ),
            ));
        }
        build_interface_info_recursive(
            parent_name,
            interface_map,
            class_map,
            checker,
            next_interface_id,
            building,
        )?;
        let parent_info = checker
            .interfaces
            .get(parent_name)
            .cloned()
            .ok_or_else(|| {
                CompileError::new(
                    interface.span,
                    &format!("Unknown parent interface: {}", parent_name),
                )
            })?;
        for method_name in &parent_info.method_order {
            let parent_sig = parent_info
                .methods
                .get(method_name)
                .expect("type checker bug: missing interface parent method signature");
            if let Some(existing_sig) = methods.get(method_name) {
                validate_signature_compatibility(
                    interface.span,
                    &interface.name,
                    method_name,
                    existing_sig,
                    parent_sig,
                    "method",
                    "combining interface parent",
                )?;
                continue;
            }
            methods.insert(method_name.clone(), parent_sig.clone());
            let declaring = parent_info
                .method_declaring_interfaces
                .get(method_name)
                .cloned()
                .unwrap_or_else(|| parent_name.clone());
            method_declaring_interfaces.insert(method_name.clone(), declaring);
            let slot = method_order.len();
            method_slots.insert(method_name.clone(), slot);
            method_order.push(method_name.clone());
        }
        for property_name in &parent_info.property_order {
            let parent_contract = parent_info
                .properties
                .get(property_name)
                .expect("type checker bug: missing interface parent property contract");
            if let Some(existing_contract) = properties.get_mut(property_name) {
                merge_property_contract(
                    existing_contract,
                    parent_contract,
                    checker,
                    interface.span,
                    &interface.name,
                    property_name,
                    "combining interface parent",
                )?;
                continue;
            }
            properties.insert(property_name.clone(), parent_contract.clone());
            property_order.push(property_name.clone());
        }
    }

    let mut direct_property_names = HashSet::new();
    for property in &interface.properties {
        if !direct_property_names.insert(property.name.clone()) {
            return Err(CompileError::new(
                property.span,
                &format!(
                    "Duplicate interface property declaration: {}::${}",
                    interface.name, property.name
                ),
            ));
        }
        validate_interface_property_syntax(&interface.name, property)?;
        let contract = build_property_contract(checker, &interface.name, property)?;
        if let Some(existing_contract) = properties.get_mut(&property.name) {
            merge_property_contract(
                existing_contract,
                &contract,
                checker,
                property.span,
                &interface.name,
                &property.name,
                "redeclaring interface",
            )?;
        } else {
            properties.insert(property.name.clone(), contract);
            property_order.push(property.name.clone());
        }
    }

    let mut direct_method_keys = HashSet::new();
    for method in &interface.methods {
        let method_key = php_symbol_key(&method.name);
        if !direct_method_keys.insert(method_key.clone()) {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Duplicate interface method declaration: {}::{}",
                    interface.name, method.name
                ),
            ));
        }
        if method.visibility != Visibility::Public {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Interface methods must be public: {}::{}",
                    interface.name, method.name
                ),
            ));
        }
        if method.is_final {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Interface method {}::{} must not be final",
                    interface.name, method.name
                ),
            ));
        }
        if method.is_static {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Static interface methods are not supported yet: {}::{}",
                    interface.name, method.name
                ),
            ));
        }
        if method.has_body {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Interface methods cannot have a body: {}::{}",
                    interface.name, method.name
                ),
            ));
        }

        let sig = build_method_sig(checker, method)?;
        if let Some(parent_sig) = methods.get(&method_key) {
            validate_signature_compatibility(
                method.span,
                &interface.name,
                &method.name,
                &sig,
                parent_sig,
                "method",
                "redeclaring interface",
            )?;
        }
        methods.insert(method_key.clone(), sig);
        method_declaring_interfaces.insert(method_key.clone(), interface.name.clone());
        if !method_slots.contains_key(&method_key) {
            let slot = method_order.len();
            method_slots.insert(method_key.clone(), slot);
            method_order.push(method_key.clone());
        }
    }

    let mut iface_constants: HashMap<String, crate::parser::ast::Expr> = HashMap::new();
    for parent_name in &interface.extends {
        if let Some(parent_info) = checker.interfaces.get(parent_name) {
            for (k, v) in &parent_info.constants {
                iface_constants
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
        }
    }
    for c in &interface.constants {
        iface_constants.insert(c.name.clone(), c.value.clone());
    }
    checker.interfaces.insert(
        interface.name.clone(),
        InterfaceInfo {
            interface_id: *next_interface_id,
            parents: interface.extends.clone(),
            properties,
            property_order,
            methods,
            method_declaring_interfaces,
            method_order,
            method_slots,
            constants: iface_constants,
        },
    );
    *next_interface_id += 1;
    building.remove(interface_name);
    Ok(())
}

/// Validates a single interface property declaration for syntactic correctness.
/// Checks that the property is public, non-static, non-readonly, and has at least one hook.
///
/// Returns `Ok(())` if the property is valid, or a `CompileError` describing the violation.
fn validate_interface_property_syntax(
    interface_name: &str,
    property: &ClassProperty,
) -> Result<(), CompileError> {
    if property.visibility != Visibility::Public {
        return Err(CompileError::new(
            property.span,
            &format!(
                "Interface properties must be public: {}::${}",
                interface_name, property.name
            ),
        ));
    }
    if property.is_static {
        return Err(CompileError::new(
            property.span,
            &format!(
                "Interface property {}::${} cannot be static",
                interface_name, property.name
            ),
        ));
    }
    if property.readonly {
        return Err(CompileError::new(
            property.span,
            &format!(
                "Hooked properties cannot be readonly: {}::${}",
                interface_name, property.name
            ),
        ));
    }
    if !property.hooks.any() {
        return Err(CompileError::new(
            property.span,
            &format!(
                "Interfaces may only include hooked properties: {}::${}",
                interface_name, property.name
            ),
        ));
    }
    Ok(())
}

/// Builds a `PropertyHookContract` for a single interface property declaration.
/// Resolves the declared type hint (or `PhpType::Mixed` if none) and extracts get/set hook
/// requirements and by-ref semantics from the property's hook list.
///
/// Returns the contract or a `CompileError` if type resolution fails.
pub(crate) fn build_property_contract(
    checker: &Checker,
    declaring_type: &str,
    property: &ClassProperty,
) -> Result<PropertyHookContract, CompileError> {
    let property_ty = match property.type_expr.as_ref() {
        Some(type_expr) => checker.resolve_declared_property_type_hint(
            type_expr,
            property.span,
            &format!("Property {}::${}", declaring_type, property.name),
        )?,
        None => PhpType::Mixed,
    };
    Ok(PropertyHookContract {
        get_type: property
            .hooks
            .requires_get()
            .then(|| property_ty.clone()),
        set_type: property.hooks.set.then_some(property_ty),
        get_by_ref: property.hooks.get_by_ref,
        declaring_type: declaring_type.to_string(),
        span: property.span,
    })
}

/// Merges an incoming property contract into an existing one, checking type compatibility
/// for both get and set hooks. When types are mutually compatible, the incoming contract
/// widens the existing one. When they are incompatible, returns an error.
///
/// `context` is used in error messages to describe where the merge is happening (e.g.,
/// "combining interface parent" or "redeclaring interface").
pub(crate) fn merge_property_contract(
    existing: &mut PropertyHookContract,
    incoming: &PropertyHookContract,
    checker: &Checker,
    span: crate::span::Span,
    owner_name: &str,
    property_name: &str,
    context: &str,
) -> Result<(), CompileError> {
    if let Some(incoming_get) = incoming.get_type.as_ref() {
        match existing.get_type.as_ref() {
            Some(existing_get) if checker.type_accepts(existing_get, incoming_get) => {
                existing.get_type = Some(incoming_get.clone());
                existing.get_by_ref |= incoming.get_by_ref;
                existing.span = incoming.span;
            }
            Some(existing_get) if checker.type_accepts(incoming_get, existing_get) => {
                existing.get_by_ref |= incoming.get_by_ref;
            }
            Some(existing_get) => {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Incompatible get property contract when {}: {}::${} requires {}, conflicting with {}",
                        context, owner_name, property_name, incoming_get, existing_get
                    ),
                ))
            }
            None => {
                existing.get_type = Some(incoming_get.clone());
                existing.get_by_ref = incoming.get_by_ref;
                existing.span = incoming.span;
            }
        }
    }
    if let Some(incoming_set) = incoming.set_type.as_ref() {
        match existing.set_type.as_ref() {
            Some(existing_set) if checker.type_accepts(existing_set, incoming_set) => {}
            Some(existing_set) if checker.type_accepts(incoming_set, existing_set) => {
                existing.set_type = Some(incoming_set.clone());
                existing.span = incoming.span;
            }
            Some(existing_set) => {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Incompatible set property contract when {}: {}::${} requires {}, conflicting with {}",
                        context, owner_name, property_name, incoming_set, existing_set
                    ),
                ))
            }
            None => {
                existing.set_type = Some(incoming_set.clone());
                existing.span = incoming.span;
            }
        }
    }
    Ok(())
}
