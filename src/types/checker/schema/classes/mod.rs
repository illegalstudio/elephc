//! Purpose:
//! Builds flattened class schema metadata from parsed declarations and inherited members.
//! Coordinates property, method, interface, and state validation for class declarations.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Flattening must preserve visibility, overrides, readonly/final constraints, and interface obligations.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;
use crate::types::ClassInfo;

mod constants;
mod interfaces;
mod methods;
mod properties;
mod state;

use super::super::Checker;
use super::validation::build_constructor_param_map;
use state::ClassBuildState;

pub(crate) fn build_class_info_recursive(
    class_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if checker.classes.contains_key(class_name) {
        return Ok(());
    }

    if !building.insert(class_name.to_string()) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Circular inheritance detected involving class {}",
                class_name
            ),
        ));
    }

    let class = load_class(class_name, class_map)?;
    validate_class_modifiers(&class)?;
    let parent_info = resolve_parent_info(
        class_name,
        &class,
        class_map,
        checker,
        next_class_id,
        building,
    )?;
    validate_parent_constraints(&class, parent_info.as_ref())?;

    let mut state = ClassBuildState::from_parent(parent_info.as_ref());
    properties::apply_properties(&mut state, &class, checker)?;
    methods::apply_methods(&mut state, &class, checker)?;
    interfaces::collect_interfaces(&mut state, &class, class_map, checker)?;
    interfaces::validate_interface_contracts(
        &mut state,
        &class,
        class_map,
        checker,
        next_class_id,
        building,
    )?;
    interfaces::ensure_concrete_class_implements_abstracts(&state, &class)?;

    let constructor_param_to_prop =
        constructor_param_to_prop_for(&class, parent_info.as_ref());
    let class_info = state.into_class_info(*next_class_id, &class, constructor_param_to_prop)?;
    checker.classes.insert(class.name.clone(), class_info);
    *next_class_id += 1;
    building.remove(class_name);
    Ok(())
}

fn load_class(
    class_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
) -> Result<FlattenedClass, CompileError> {
    class_map.get(class_name).cloned().ok_or_else(|| {
        CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Unknown class referenced during inheritance flattening: {}",
                class_name
            ),
        )
    })
}

fn validate_class_modifiers(class: &FlattenedClass) -> Result<(), CompileError> {
    if class.is_abstract && class.is_final {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "Cannot use the final modifier on an abstract class",
        ));
    }
    Ok(())
}

fn resolve_parent_info(
    class_name: &str,
    class: &FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<Option<ClassInfo>, CompileError> {
    let Some(parent_name) = &class.extends else {
        return Ok(None);
    };
    if checker.interfaces.contains_key(parent_name) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Class {} cannot extend interface {}; use implements instead",
                class_name, parent_name
            ),
        ));
    }
    build_class_info_recursive(parent_name, class_map, checker, next_class_id, building)?;
    checker
        .classes
        .get(parent_name)
        .cloned()
        .map(Some)
        .ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown parent class: {}", parent_name),
            )
        })
}

fn validate_parent_constraints(
    class: &FlattenedClass,
    parent_info: Option<&ClassInfo>,
) -> Result<(), CompileError> {
    let (Some(parent), Some(parent_name)) = (parent_info, class.extends.as_ref()) else {
        return Ok(());
    };
    if parent.is_final {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!("Class {} cannot extend final class {}", class.name, parent_name),
        ));
    }
    if class.is_readonly_class != parent.is_readonly_class {
        let relation = if class.is_readonly_class {
            "readonly class cannot extend non-readonly parent"
        } else {
            "non-readonly class cannot extend readonly parent"
        };
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!("{}: {} extends {}", relation, class.name, parent_name),
        ));
    }
    Ok(())
}

fn constructor_param_to_prop_for(
    class: &FlattenedClass,
    parent_info: Option<&ClassInfo>,
) -> Vec<Option<String>> {
    if class
        .methods
        .iter()
        .any(|m| php_symbol_key(&m.name) == "__construct")
    {
        build_constructor_param_map(&class.methods)
    } else if let Some(parent) = parent_info {
        parent.constructor_param_to_prop.clone()
    } else {
        Vec::new()
    }
}
