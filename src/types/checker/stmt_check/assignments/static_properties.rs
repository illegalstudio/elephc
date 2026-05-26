//! Purpose:
//! Type-checks assignment static properties forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, StaticReceiver};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

/// Internal data for static property assignment resolution.
/// Holds the resolved class, declaring class, declared-type status, and current property type.
struct StaticPropertyAssignmentTarget {
    class_name: String,
    declaring_class: String,
    property_has_declared_type: bool,
    prop_ty: PhpType,
}

/// Type-checks a direct static property assignment `Class::$prop = value`.
///
/// Infers the value type, resolves the property target via `resolve_static_property_assignment_target`,
/// validates type compatibility against declared types, and refines the property type when no
/// declared type is present.
pub(super) fn check_static_property_assign(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;

    if target.property_has_declared_type {
        checker.require_compatible_arg_type(
            &target.prop_ty,
            &val_ty,
            span,
            &format!("Static property {}::${}", target.class_name, property),
        )?;
    }

    if !target.property_has_declared_type {
        refine_static_property_assignment_type(
            checker,
            property,
            &target.declaring_class,
            &val_ty,
        );
    }
    Ok(())
}

/// Type-checks an array-push assignment `Class::$prop[] = value`.
///
/// Infers the value type, resolves the property target, validates element-type compatibility
/// against declared types, merges element types when the property is untyped, and updates the
/// property type. Rejects buffer properties and non-array static properties.
pub(super) fn check_static_property_array_push(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    let updated_prop_ty = match target.prop_ty {
        PhpType::Array(elem_ty) => {
            if target.property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    &val_ty,
                    span,
                    &format!("Static property {}::${}[]", target.class_name, property),
                )?;
                PhpType::Array(elem_ty)
            } else if *elem_ty == val_ty {
                PhpType::Array(elem_ty)
            } else {
                let merged_ty = checker
                    .merge_array_element_type(&elem_ty, &val_ty)
                    .unwrap_or(PhpType::Mixed);
                PhpType::Array(Box::new(merged_ty))
            }
        }
        PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
            PhpType::Array(Box::new(val_ty.clone()))
        }
        PhpType::Buffer(_) => {
            return Err(CompileError::new(
                span,
                "buffer<T> does not support push; allocate with buffer_new<T>(len)",
            ))
        }
        other => {
            return Err(CompileError::new(
                span,
                &format!("Array push requires an array static property, got {}", other),
            ))
        }
    };

    if !target.property_has_declared_type {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            updated_prop_ty,
        );
    }
    Ok(())
}

/// Type-checks an indexed array assignment `Class::$prop[index] = value`.
///
/// Infers the index and value types, resolves the property target, validates integer index,
/// validates element-type compatibility against declared types, merges element types when the
/// property is untyped, and updates the property type. Short-circuits for `ArrayAccess` objects.
pub(super) fn check_static_property_array_assign(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    if let PhpType::Object(class_name) = &target.prop_ty {
        if checker.object_type_implements_interface(class_name, "ArrayAccess") {
            return Ok(());
        }
    }
    if idx_ty != PhpType::Int {
        return Err(CompileError::new(span, "Array index must be integer"));
    }

    let updated_prop_ty = match target.prop_ty {
        PhpType::Array(elem_ty) => {
            if target.property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    &val_ty,
                    span,
                    &format!("Static property {}::${}[]", target.class_name, property),
                )?;
                PhpType::Array(elem_ty)
            } else if *elem_ty == val_ty {
                PhpType::Array(elem_ty)
            } else {
                let merged_ty = checker
                    .merge_array_element_type(&elem_ty, &val_ty)
                    .unwrap_or(PhpType::Mixed);
                PhpType::Array(Box::new(merged_ty))
            }
        }
        PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
            PhpType::Array(Box::new(val_ty.clone()))
        }
        other => {
            return Err(CompileError::new(
                span,
                &format!(
                    "Array index assignment requires an array static property, got {}",
                    other
                ),
            ))
        }
    };

    if !target.property_has_declared_type {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            updated_prop_ty,
        );
    }
    Ok(())
}

/// Resolves `receiver` to a class name and fetches static property metadata.
///
/// Returns `StaticPropertyAssignmentTarget` with class name, declaring class,
/// declared-type flag, and current property type. Checks that the property exists
/// and that the current context can access it per PHP visibility rules.
fn resolve_static_property_assignment_target(
    checker: &Checker,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
) -> Result<StaticPropertyAssignmentTarget, CompileError> {
    let class_name = match receiver {
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
        StaticReceiver::Self_ => checker
            .current_class
            .as_ref()
            .cloned()
            .ok_or_else(|| CompileError::new(span, "Cannot use self:: outside class method scope"))?,
        StaticReceiver::Static => checker
            .current_class
            .as_ref()
            .cloned()
            .ok_or_else(|| CompileError::new(span, "Cannot use static:: outside class method scope"))?,
        StaticReceiver::Parent => {
            let current_class = checker.current_class.as_ref().ok_or_else(|| {
                CompileError::new(span, "Cannot use parent:: outside class method scope")
            })?;
            let current_info = checker.classes.get(current_class).ok_or_else(|| {
                CompileError::new(span, &format!("Undefined class: {}", current_class))
            })?;
            current_info.parent.as_ref().cloned().ok_or_else(|| {
                CompileError::new(
                    span,
                    &format!("Class {} has no parent class", current_class),
                )
            })?
        }
    };

    let class_info = checker
        .classes
        .get(&class_name)
        .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", class_name)))?;
    if !class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return Err(CompileError::new(
            span,
            &format!("Undefined static property: {}::{}", class_name, property),
        ));
    }
    if let Some(visibility) = class_info.static_property_visibilities.get(property) {
        let declaring_class = class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        if !checker.can_access_member(declaring_class, visibility) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot access {} static property: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    property
                ),
            ));
        }
    }
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let property_has_declared_type = class_info.declared_static_properties.contains(property);
    let prop_ty = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int);

    Ok(StaticPropertyAssignmentTarget {
        class_name,
        declaring_class,
        property_has_declared_type,
        prop_ty,
    })
}

/// Updates the type of a static property in all classes that declare it.
///
/// Iterates over all classes and mutates the type entry for `property` on the
/// declaring class `declaring_class`. Used after array-push or index assignment
/// to propagate the new element type.
fn update_static_property_type(
    checker: &mut Checker,
    property: &str,
    declaring_class: &str,
    updated_ty: PhpType,
) {
    for class_info in checker.classes.values_mut() {
        if class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            != Some(declaring_class)
        {
            continue;
        }
        if let Some(prop) = class_info
            .static_properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            prop.1 = updated_ty.clone();
        }
    }
}

/// Refines the type of a static property based on an assigned value.
///
/// If the property currently holds `Int` or `Void` (uninitialized), it is set to `val_ty`.
/// Otherwise `specialize_generic_array_hint` is used to merge the value type into the
/// existing array element type. Only updates when the refined type differs from the current.
fn refine_static_property_assignment_type(
    checker: &mut Checker,
    property: &str,
    declaring_class: &str,
    val_ty: &PhpType,
) {
    for class_info in checker.classes.values_mut() {
        if class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            != Some(declaring_class)
        {
            continue;
        }
        if let Some(prop) = class_info
            .static_properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            if matches!(prop.1, PhpType::Int | PhpType::Void) && prop.1 != *val_ty {
                prop.1 = val_ty.clone();
            } else {
                let refined_ty = Checker::specialize_generic_array_hint(&prop.1, val_ty);
                if refined_ty != prop.1 {
                    prop.1 = refined_ty;
                }
            }
        }
    }
}
