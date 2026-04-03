use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::Visibility;
use crate::types::InterfaceInfo;

use super::super::Checker;
use super::super::InterfaceDeclInfo;
use super::validation::{build_method_sig, validate_signature_compatibility};
use crate::types::traits::FlattenedClass;

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
    }

    for method in &interface.methods {
        if method.visibility != Visibility::Public {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Interface methods must be public: {}::{}",
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
        if let Some(parent_sig) = methods.get(&method.name) {
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
        methods.insert(method.name.clone(), sig);
        method_declaring_interfaces.insert(method.name.clone(), interface.name.clone());
        if !method_slots.contains_key(&method.name) {
            let slot = method_order.len();
            method_slots.insert(method.name.clone(), slot);
            method_order.push(method.name.clone());
        }
    }

    checker.interfaces.insert(
        interface.name.clone(),
        InterfaceInfo {
            interface_id: *next_interface_id,
            parents: interface.extends.clone(),
            methods,
            method_declaring_interfaces,
            method_order,
            method_slots,
        },
    );
    *next_interface_id += 1;
    building.remove(interface_name);
    Ok(())
}
