use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

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
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    match &obj_ty {
        PhpType::Object(class_name) => {
            let (prop_ty, property_has_declared_type) =
                resolve_object_array_property(checker, class_name, property, span)?;
            if idx_ty != PhpType::Int {
                return Err(CompileError::new(span, "Array index must be integer"));
            }

            let updated_prop_ty = updated_array_property_assign_type(
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
            let field_ty = resolve_pointer_field_type(
                checker,
                class_name,
                property,
                span,
                "Array index assignment",
            )?;

            if idx_ty != PhpType::Int {
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
    if let Some(class_info) = checker.classes.get(class_name) {
        if !class_info.properties.iter().any(|(n, _)| n == property) {
            if class_info.methods.contains_key("__set") {
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

fn type_can_be_null(ty: &PhpType) -> bool {
    *ty == PhpType::Void || Checker::union_contains_void(ty) || matches!(ty, PhpType::Mixed)
}

fn null_coalesce_property_keeps_non_null(
    object: &Expr,
    property: &str,
    value: &Expr,
    property_ty: &PhpType,
) -> bool {
    if type_can_be_null(property_ty) {
        return false;
    }
    let ExprKind::NullCoalesce {
        value: current,
        default: _,
    } = &value.kind
    else {
        return false;
    };
    let ExprKind::PropertyAccess {
        object: current_object,
        property: current_property,
    } = &current.kind
    else {
        return false;
    };
    current_property == property && assignment_expr_equivalent(current_object, object)
}

fn assignment_expr_equivalent(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(a), ExprKind::Variable(b)) => a == b,
        (ExprKind::This, ExprKind::This) => true,
        (
            ExprKind::PropertyAccess {
                object: a_object,
                property: a_property,
            },
            ExprKind::PropertyAccess {
                object: b_object,
                property: b_property,
            },
        ) => a_property == b_property && assignment_expr_equivalent(a_object, b_object),
        _ => false,
    }
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
                    .unwrap_or_else(|| val_ty.clone());
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
                    .unwrap_or_else(|| val_ty.clone());
                Ok(PhpType::Array(Box::new(merged_ty)))
            }
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

fn update_object_property_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    property_has_declared_type: bool,
    updated_prop_ty: PhpType,
) {
    if let Some(class_info) = checker.classes.get_mut(class_name) {
        if !property_has_declared_type {
            if let Some(prop) = class_info
                .properties
                .iter_mut()
                .find(|(name, _)| name == property)
            {
                prop.1 = updated_prop_ty;
            }
        }
    }
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
