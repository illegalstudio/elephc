use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::{merge_array_key_types, normalized_array_key_type, PhpType, TypeEnv};

use super::super::super::Checker;

pub(super) fn check_array_assign(
    checker: &mut Checker,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let arr_ty = env
        .get(array)
        .cloned()
        .ok_or_else(|| CompileError::new(span, &format!("Undefined variable: ${}", array)))?;
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    if arr_ty == PhpType::Str {
        return Err(CompileError::new(
            span,
            "String offset assignment is not supported",
        ));
    }
    if let PhpType::Array(elem_ty) = &arr_ty {
        if **elem_ty != val_ty {
            let merged_ty = checker
                .merge_array_element_type(elem_ty, &val_ty)
                .unwrap_or(val_ty);
            env.insert(array.to_string(), PhpType::Array(Box::new(merged_ty)));
        }
    } else if let PhpType::AssocArray {
        key,
        value: existing_value,
    } = &arr_ty
    {
        let merged_key = merge_array_key_types(
            *key.clone(),
            normalized_array_key_type(index, idx_ty),
        );
        let merged_value = if **existing_value == val_ty {
            *existing_value.clone()
        } else {
            PhpType::Mixed
        };
        env.insert(
            array.to_string(),
            PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            },
        );
    } else if let PhpType::Buffer(elem_ty) = &arr_ty {
        if !matches!(idx_ty, PhpType::Int) {
            return Err(CompileError::new(span, "Buffer index must be integer"));
        }
        match elem_ty.as_ref() {
            PhpType::Packed(_) => {
                return Err(CompileError::new(
                    span,
                    "Assign packed buffer elements through field access like $buf[$i]->field",
                ))
            }
            inner if inner != &val_ty => {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Buffer element type mismatch: expected {:?}, got {:?}",
                        inner, val_ty
                    ),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn check_array_push(
    checker: &mut Checker,
    array: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let arr_ty = env
        .get(array)
        .cloned()
        .ok_or_else(|| CompileError::new(span, &format!("Undefined variable: ${}", array)))?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    if let PhpType::Array(elem_ty) = &arr_ty {
        if **elem_ty != val_ty {
            let merged_ty = checker
                .merge_array_element_type(elem_ty, &val_ty)
                .unwrap_or(val_ty);
            env.insert(array.to_string(), PhpType::Array(Box::new(merged_ty)));
        }
    } else if matches!(arr_ty, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            span,
            "buffer<T> does not support push; allocate with buffer_new<T>(len)",
        ));
    }
    Ok(())
}
