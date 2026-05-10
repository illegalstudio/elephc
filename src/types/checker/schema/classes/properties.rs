//! Purpose:
//! Validates class schema properties rules.
//! Owns one slice of class metadata construction used by object inference and method checking.
//!
//! Called from:
//! - `crate::types::checker::schema::classes`
//!
//! Key details:
//! - Class metadata is shared globally after construction, so validation must reject inconsistent inheritance early.

use crate::errors::CompileError;
use crate::parser::ast::{ClassProperty, Visibility};
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::super::super::{infer_expr_type_syntactic, Checker};
use super::super::validation::visibility_rank;
use super::state::ClassBuildState;

pub(super) fn apply_properties(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
) -> Result<(), CompileError> {
    for prop in &class.properties {
        if prop.is_static {
            apply_static_property(state, class, checker, prop)?;
        } else {
            apply_instance_property(state, class, checker, prop)?;
        }
    }
    Ok(())
}

fn apply_static_property(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    prop: &ClassProperty,
) -> Result<(), CompileError> {
    if prop.by_ref {
        return Err(CompileError::new(
            prop.span,
            "Static by-reference properties are not supported",
        ));
    }
    if prop.is_final && prop.visibility == Visibility::Private {
        return Err(CompileError::new(
            prop.span,
            "Property cannot be both final and private",
        ));
    }
    if state.property_declaring_classes.contains_key(&prop.name) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot redeclare instance property as static property: {}::{}",
                class.name, prop.name
            ),
        ));
    }

    let inherited_static_declaring_class =
        state.static_property_declaring_classes.get(&prop.name).cloned();
    let declared_ty = resolve_property_declared_type(checker, &class.name, prop)?;
    if let Some(parent_declaring_class) = inherited_static_declaring_class.as_ref() {
        validate_static_property_override(state, class, prop, declared_ty.as_ref(), parent_declaring_class)?;
    }

    let ty = if let Some(declared_ty) = declared_ty {
        checker.validate_declared_default_type(
            &declared_ty,
            prop.default.as_ref(),
            prop.span,
            &format!("Static property {}::${} default", class.name, prop.name),
        )?;
        state.declared_static_properties.insert(prop.name.clone());
        declared_ty
    } else if let Some(default) = &prop.default {
        state.declared_static_properties.remove(&prop.name);
        infer_expr_type_syntactic(default)
    } else {
        state.declared_static_properties.remove(&prop.name);
        PhpType::Int
    };

    if let Some(slot) = state
        .static_prop_types
        .iter()
        .position(|(name, _)| name == &prop.name)
    {
        state.static_prop_types[slot] = (prop.name.clone(), ty);
        state.static_defaults[slot] = prop.default.clone();
    } else {
        state.static_prop_types.push((prop.name.clone(), ty));
        state.static_defaults.push(prop.default.clone());
    }
    state
        .static_property_declaring_classes
        .insert(prop.name.clone(), class.name.clone());
    state
        .static_property_visibilities
        .insert(prop.name.clone(), prop.visibility.clone());
    if prop.is_final {
        state.final_static_properties.insert(prop.name.clone());
    } else {
        state.final_static_properties.remove(&prop.name);
    }
    Ok(())
}

fn validate_static_property_override(
    state: &ClassBuildState,
    class: &FlattenedClass,
    prop: &ClassProperty,
    declared_ty: Option<&PhpType>,
    parent_declaring_class: &str,
) -> Result<(), CompileError> {
    let inherited_visibility = state
        .static_property_visibilities
        .get(&prop.name)
        .cloned()
        .unwrap_or(Visibility::Public);
    let inherited_is_private = inherited_visibility == Visibility::Private;
    if state.final_static_properties.contains(&prop.name) {
        let declaring_class = state
            .static_property_declaring_classes
            .get(&prop.name)
            .cloned()
            .unwrap_or_else(|| class.name.clone());
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot override final static property {}::${}",
                declaring_class, prop.name
            ),
        ));
    }
    if inherited_is_private {
        return Ok(());
    }
    if visibility_rank(&prop.visibility) < visibility_rank(&inherited_visibility) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot reduce visibility when overriding static property: {}::{}",
                class.name, prop.name
            ),
        ));
    }

    let parent_declared = state.declared_static_properties.contains(&prop.name);
    match (parent_declared, declared_ty) {
        (true, None) => {
            let parent_ty = inherited_static_property_type(state, &prop.name);
            Err(CompileError::new(
                prop.span,
                &format!(
                    "Type of {}::${} must be {} (as in class {})",
                    class.name, prop.name, parent_ty, parent_declaring_class
                ),
            ))
        }
        (false, Some(_)) => Err(CompileError::new(
            prop.span,
            &format!(
                "Type of {}::${} must not be defined (as in class {})",
                class.name, prop.name, parent_declaring_class
            ),
        )),
        (true, Some(child_ty)) => {
            let parent_ty = inherited_static_property_type(state, &prop.name);
            if &parent_ty != child_ty {
                return Err(CompileError::new(
                    prop.span,
                    &format!(
                        "Type of {}::${} must be {} (as in class {})",
                        class.name, prop.name, parent_ty, parent_declaring_class
                    ),
                ));
            }
            Ok(())
        }
        (false, None) => Ok(()),
    }
}

fn apply_instance_property(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    prop: &ClassProperty,
) -> Result<(), CompileError> {
    if prop.is_final && prop.visibility == Visibility::Private {
        return Err(CompileError::new(
            prop.span,
            "Property cannot be both final and private",
        ));
    }
    if state.static_property_declaring_classes.contains_key(&prop.name) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot redeclare static property as instance property: {}::{}",
                class.name, prop.name
            ),
        ));
    }
    if prop.by_ref && class.is_readonly_class {
        return Err(CompileError::new(
            prop.span,
            "Readonly promoted by-reference properties are not supported",
        ));
    }
    if state.property_declaring_classes.contains_key(&prop.name) {
        if state.final_properties.contains(&prop.name) {
            let declaring_class = state
                .property_declaring_classes
                .get(&prop.name)
                .cloned()
                .unwrap_or_else(|| class.name.clone());
            return Err(CompileError::new(
                prop.span,
                &format!(
                    "Cannot override final property {}::${}",
                    declaring_class, prop.name
                ),
            ));
        }
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Property redeclaration across inheritance is not yet supported: {}::{}",
                class.name, prop.name
            ),
        ));
    }

    let ty = if let Some(declared_ty) = resolve_property_declared_type(checker, &class.name, prop)? {
        checker.validate_declared_default_type(
            &declared_ty,
            prop.default.as_ref(),
            prop.span,
            &format!("Property {}::${} default", class.name, prop.name),
        )?;
        state.declared_properties.insert(prop.name.clone());
        declared_ty
    } else if let Some(default) = &prop.default {
        infer_expr_type_syntactic(default)
    } else {
        PhpType::Int
    };

    let slot_index = state.prop_types.len();
    state.prop_types.push((prop.name.clone(), ty));
    state
        .property_offsets
        .insert(prop.name.clone(), 8 + slot_index * 16);
    state
        .property_declaring_classes
        .insert(prop.name.clone(), class.name.clone());
    state.defaults.push(prop.default.clone());
    state
        .property_visibilities
        .insert(prop.name.clone(), prop.visibility.clone());
    if prop.is_final {
        state.final_properties.insert(prop.name.clone());
    } else {
        state.final_properties.remove(&prop.name);
    }
    if class.is_readonly_class || prop.readonly {
        state.readonly_properties.insert(prop.name.clone());
    }
    if prop.by_ref {
        state.reference_properties.insert(prop.name.clone());
    }
    Ok(())
}

fn inherited_static_property_type(state: &ClassBuildState, property: &str) -> PhpType {
    state
        .static_prop_types
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int)
}

fn resolve_property_declared_type(
    checker: &Checker,
    class_name: &str,
    prop: &ClassProperty,
) -> Result<Option<PhpType>, CompileError> {
    prop.type_expr
        .as_ref()
        .map(|type_expr| {
            checker.resolve_declared_property_type_hint(
                type_expr,
                prop.span,
                &format!("Property {}::${}", class_name, prop.name),
            )
        })
        .transpose()
}
