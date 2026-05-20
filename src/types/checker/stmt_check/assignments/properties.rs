//! Purpose:
//! Type-checks assignment properties forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::{
    merge_array_key_types, normalized_array_key_type, static_array_key_forces_hash_storage,
    PhpType, TypeEnv,
};

use super::super::super::Checker;
use super::properties_null_coalesce::null_coalesce_property_keeps_non_null;

pub(super) fn check_property_assign(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    if let PhpType::Object(class_name) = &obj_ty {
        check_object_property_write(checker, object, class_name, property, value, &val_ty, span)?;
        refine_object_property_type(checker, class_name, property, &val_ty);
    }
    if let PhpType::Pointer(Some(class_name)) = &obj_ty {
        check_pointer_property_write(checker, class_name, property, &val_ty, span)?;
    }
    Ok(())
}

pub(super) fn check_property_array_push(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    match &obj_ty {
        PhpType::Object(class_name) => {
            let (prop_ty, property_has_declared_type) =
                resolve_object_array_property(checker, class_name, property, span)?;
            let updated_prop_ty = updated_array_property_push_type(
                checker,
                &prop_ty,
                property_has_declared_type,
                class_name,
                property,
                &val_ty,
                span,
            )?;
            update_object_property_type(
                checker,
                class_name,
                property,
                property_has_declared_type,
                updated_prop_ty,
            );
            Ok(())
        }
        PhpType::Pointer(Some(class_name)) => {
            let field_ty = resolve_pointer_field_type(checker, class_name, property, span, "Array push")?;
            match field_ty {
                PhpType::Array(_) => Ok(()),
                PhpType::Buffer(_) => Err(CompileError::new(
                    span,
                    "buffer<T> does not support push; allocate with buffer_new<T>(len)",
                )),
                other => Err(CompileError::new(
                    span,
                    &format!("Array push requires an array property, got {}", other),
                )),
            }
        }
        _ => Err(CompileError::new(
            span,
            "Array push requires an object or typed pointer",
        )),
    }
}

pub(super) fn check_property_array_assign(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    if matches!(obj_ty, PhpType::Mixed) {
        if !is_php_array_key_type(&normalized_idx_ty) {
            return Err(CompileError::new(span, "Array index must be integer"));
        }
        return Ok(());
    }
    match &obj_ty {
        PhpType::Object(class_name) => {
            let (prop_ty, property_has_declared_type) =
                resolve_object_array_property(checker, class_name, property, span)?;
            if let PhpType::Object(prop_class_name) = &prop_ty {
                if checker.object_type_implements_interface(prop_class_name, "ArrayAccess") {
                    return Ok(());
                }
            }
            if !is_php_array_key_type(&normalized_idx_ty) {
                return Err(CompileError::new(span, "Array index must be integer"));
            }

            let updated_prop_ty = updated_array_property_assign_type(
                checker,
                &prop_ty,
                property_has_declared_type,
                class_name,
                property,
                index,
                &normalized_idx_ty,
                &val_ty,
                span,
            )?;
            update_object_property_type(
                checker,
                class_name,
                property,
                property_has_declared_type,
                updated_prop_ty,
            );
            Ok(())
        }
        PhpType::Pointer(Some(class_name)) => {
            let field_ty = resolve_pointer_field_type(
                checker,
                class_name,
                property,
                span,
                "Array index assignment",
            )?;

            if !matches!(normalized_idx_ty, PhpType::Int) {
                return Err(CompileError::new(span, "Array index must be integer"));
            }

            match field_ty {
                PhpType::Array(_) => Ok(()),
                other => Err(CompileError::new(
                    span,
                    &format!(
                        "Array index assignment requires an array property, got {}",
                        other
                    ),
                )),
            }
        }
        _ => Err(CompileError::new(
            span,
            "Array index assignment requires an object or typed pointer",
        )),
    }
}

fn check_object_property_write(
    checker: &Checker,
    object: &Expr,
    class_name: &str,
    property: &str,
    value: &Expr,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return Ok(());
    }
    if let Some(class_info) = checker.classes.get(class_name) {
        if !class_info.properties.iter().any(|(n, _)| n == property) {
            if class_info.methods.contains_key("__set") {
                return Ok(());
            }
            if class_info.allow_dynamic_properties {
                // PHP 8.2 #[\AllowDynamicProperties]: writes to undeclared
                // properties are routed at codegen time to a per-object
                // hashtable side-table. The value is stored as `Mixed`.
                return Ok(());
            }
            return Err(CompileError::new(
                span,
                &format!("Undefined property: {}::{}", class_name, property),
            ));
        }
        validate_object_property_access(checker, class_name, property, span)?;
        let expected_ty = class_info
            .properties
            .iter()
            .find(|(n, _)| n == property)
            .map(|(_, ty)| ty.clone())
            .unwrap_or(PhpType::Int);
        let readonly_non_null_coalesce_keep =
            null_coalesce_property_keeps_non_null(object, property, value, &expected_ty);
        if class_info.readonly_properties.contains(property)
            && !(checker.current_class.as_deref()
                == class_info
                    .property_declaring_classes
                    .get(property)
                    .map(String::as_str)
                && checker.current_method.as_deref() == Some("__construct"))
            && !readonly_non_null_coalesce_keep
        {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot assign to readonly property outside constructor: {}::{}",
                    class_name, property
                ),
            ));
        }
        if class_info.declared_properties.contains(property) {
            checker.require_compatible_arg_type(
                &expected_ty,
                val_ty,
                span,
                &format!("Property {}::${}", class_name, property),
            )?;
        }
    }
    Ok(())
}

fn validate_object_property_access(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
) -> Result<(), CompileError> {
    let class_info = checker.classes.get(class_name).ok_or_else(|| {
        CompileError::new(span, &format!("Undefined class: {}", class_name))
    })?;
    if let Some(visibility) = class_info.property_visibilities.get(property) {
        let declaring_class = class_info
            .property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name);
        if !checker.can_access_member(declaring_class, visibility) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot access {} property: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    property
                ),
            ));
        }
    }
    Ok(())
}

fn refine_object_property_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
) {
    if let Some(class_info) = checker.classes.get_mut(class_name) {
        let property_has_declared_type = class_info.declared_properties.contains(property);
        if let Some(prop) = class_info
            .properties
            .iter_mut()
            .find(|(n, _)| n == property)
        {
            if !property_has_declared_type {
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
}

fn check_pointer_property_write(
    checker: &Checker,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    if let Some(field_ty) = checker.extern_field_type(class_name, property) {
        if field_ty == PhpType::Int && val_ty != &PhpType::Int {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot assign {:?} to extern field {}::{} of type {:?}",
                    val_ty, class_name, property, field_ty
                ),
            ));
        }
    } else if let Some(field_ty) = checker.packed_field_type(class_name, property) {
        if &field_ty != val_ty {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot assign {:?} to packed field {}::{} of type {:?}",
                    val_ty, class_name, property, field_ty
                ),
            ));
        }
    } else if checker.extern_classes.contains_key(class_name) {
        return Err(CompileError::new(
            span,
            &format!("Undefined extern field: {}::{}", class_name, property),
        ));
    } else if checker.packed_classes.contains_key(class_name) {
        return Err(CompileError::new(
            span,
            &format!("Undefined packed field: {}::{}", class_name, property),
        ));
    }
    Ok(())
}

fn resolve_object_array_property(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
) -> Result<(PhpType, bool), CompileError> {
    let class_info = checker
        .classes
        .get(class_name)
        .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", class_name)))?;
    if !class_info.properties.iter().any(|(n, _)| n == property) {
        return Err(CompileError::new(
            span,
            &format!("Undefined property: {}::{}", class_name, property),
        ));
    }
    validate_object_property_access(checker, class_name, property, span)?;
    let property_has_declared_type = class_info.declared_properties.contains(property);
    let prop_ty = class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int);
    Ok((prop_ty, property_has_declared_type))
}

fn updated_array_property_push_type(
    checker: &Checker,
    prop_ty: &PhpType,
    property_has_declared_type: bool,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    span: Span,
) -> Result<PhpType, CompileError> {
    match prop_ty {
        PhpType::Array(elem_ty) => {
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
                Ok(PhpType::Array(elem_ty.clone()))
            } else if elem_ty.as_ref() == val_ty {
                Ok(PhpType::Array(elem_ty.clone()))
            } else {
                let merged_ty = checker
                    .merge_array_element_type(elem_ty, val_ty)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::Array(Box::new(merged_ty)))
            }
        }
        PhpType::Int | PhpType::Void if !property_has_declared_type => {
            Ok(PhpType::Array(Box::new(val_ty.clone())))
        }
        PhpType::Buffer(_) => Err(CompileError::new(
            span,
            "buffer<T> does not support push; allocate with buffer_new<T>(len)",
        )),
        other => Err(CompileError::new(
            span,
            &format!("Array push requires an array property, got {}", other),
        )),
    }
}

fn updated_array_property_assign_type(
    checker: &Checker,
    prop_ty: &PhpType,
    property_has_declared_type: bool,
    class_name: &str,
    property: &str,
    index: &Expr,
    normalized_idx_ty: &PhpType,
    val_ty: &PhpType,
    span: Span,
) -> Result<PhpType, CompileError> {
    match prop_ty {
        PhpType::Array(elem_ty) => {
            if !matches!(normalized_idx_ty, PhpType::Int)
                || (matches!(elem_ty.as_ref(), PhpType::Never)
                    && static_array_key_forces_hash_storage(index))
            {
                if property_has_declared_type {
                    checker.require_compatible_arg_type(
                        elem_ty.as_ref(),
                        val_ty,
                        span,
                        &format!("Property {}::${}[]", class_name, property),
                    )?;
                }
                return Ok(assoc_property_type_after_keyed_write(
                    checker,
                    elem_ty,
                    property_has_declared_type,
                    normalized_idx_ty,
                    val_ty,
                ));
            }
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
                Ok(PhpType::Array(elem_ty.clone()))
            } else if elem_ty.as_ref() == val_ty {
                Ok(PhpType::Array(elem_ty.clone()))
            } else {
                let merged_ty = checker
                    .merge_array_element_type(elem_ty, val_ty)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::Array(Box::new(merged_ty)))
            }
        }
        PhpType::AssocArray {
            key,
            value: existing_value,
        } => {
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    existing_value.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
            }
            let merged_key = merge_array_key_types(*key.clone(), normalized_idx_ty.clone());
            let merged_value = if property_has_declared_type || existing_value.as_ref() == val_ty {
                *existing_value.clone()
            } else {
                checker
                    .merge_array_element_type(existing_value, val_ty)
                    .unwrap_or(PhpType::Mixed)
            };
            Ok(PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            })
        }
        other => Err(CompileError::new(
            span,
            &format!(
                "Array index assignment requires an array property, got {}",
                other
            ),
        )),
    }
}

fn is_php_array_key_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Str | PhpType::Mixed)
}

fn assoc_property_type_after_keyed_write(
    checker: &Checker,
    elem_ty: &PhpType,
    property_has_declared_type: bool,
    normalized_idx_ty: &PhpType,
    val_ty: &PhpType,
) -> PhpType {
    if property_has_declared_type && matches!(elem_ty, PhpType::Mixed) {
        return PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        };
    }

    let merged_key = if matches!(elem_ty, PhpType::Never) {
        normalized_idx_ty.clone()
    } else {
        merge_array_key_types(PhpType::Int, normalized_idx_ty.clone())
    };
    let merged_value = if matches!(elem_ty, PhpType::Never) {
        val_ty.clone()
    } else if elem_ty == val_ty {
        elem_ty.clone()
    } else {
        checker
            .merge_array_element_type(elem_ty, val_ty)
            .unwrap_or(PhpType::Mixed)
    };
    PhpType::AssocArray {
        key: Box::new(merged_key),
        value: Box::new(merged_value),
    }
}

fn update_object_property_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    property_has_declared_type: bool,
    updated_prop_ty: PhpType,
) {
    if let Some(class_info) = checker.classes.get_mut(class_name) {
        if let Some(prop) = class_info
            .properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            if !property_has_declared_type
                || declared_generic_array_can_use_assoc_storage(&prop.1, &updated_prop_ty)
            {
                prop.1 = updated_prop_ty;
            }
        }
    }
}

fn declared_generic_array_can_use_assoc_storage(current: &PhpType, updated: &PhpType) -> bool {
    matches!(
        (current, updated),
        (
            PhpType::Array(elem_ty),
            PhpType::AssocArray { key: _, value }
        ) if matches!(elem_ty.as_ref(), PhpType::Mixed)
            && matches!(value.as_ref(), PhpType::Mixed)
    )
}

fn resolve_pointer_field_type(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
    operation: &str,
) -> Result<PhpType, CompileError> {
    if let Some(field_ty) = checker.extern_field_type(class_name, property) {
        Ok(field_ty)
    } else if let Some(field_ty) = checker.packed_field_type(class_name, property) {
        Ok(field_ty)
    } else if checker.extern_classes.contains_key(class_name) {
        Err(CompileError::new(
            span,
            &format!("Undefined extern field: {}::{}", class_name, property),
        ))
    } else if checker.packed_classes.contains_key(class_name) {
        Err(CompileError::new(
            span,
            &format!("Undefined packed field: {}::{}", class_name, property),
        ))
    } else {
        Err(CompileError::new(
            span,
            &format!("{} requires an object or typed pointer", operation),
        ))
    }
}
