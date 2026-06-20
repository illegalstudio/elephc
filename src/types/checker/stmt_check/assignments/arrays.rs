//! Purpose:
//! Type-checks assignment arrays forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{
    merge_array_key_types, normalized_array_key_type, static_array_key_forces_hash_storage,
    PhpType, TypeEnv,
};

use super::super::super::Checker;

/// Validates and updates the type environment for `$array[$index] = $value` assignments.
///
/// Validates that the target is not a string, merges element types for arrays/assoc-arrays,
/// checks buffer index type and element type compatibility, and requires ArrayAccess for objects.
/// Updates `env` with the merged key/value types; returns an error for invalid targets or type mismatches.
///
/// Errors:
/// - Undefined variable
/// - String offset assignment
/// - Buffer element type mismatch or packed buffer assignment via index
/// - Object assignment without ArrayAccess
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
    super::locals::update_callable_assignment_metadata(checker, array, value, &val_ty, env)?;
    if arr_ty == PhpType::Str {
        return Err(CompileError::new(
            span,
            "String offset assignment is not supported",
        ));
    }
    if let PhpType::Array(elem_ty) = &arr_ty {
        let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
        if !matches!(normalized_idx_ty, PhpType::Int)
            || (matches!(elem_ty.as_ref(), PhpType::Never)
                && static_array_key_forces_hash_storage(index))
        {
            let merged_key = if matches!(elem_ty.as_ref(), PhpType::Never) {
                normalized_idx_ty
            } else {
                merge_array_key_types(PhpType::Int, normalized_idx_ty)
            };
            let merged_value = if matches!(elem_ty.as_ref(), PhpType::Never) {
                val_ty
            } else if elem_ty.as_ref() == &val_ty {
                *elem_ty.clone()
            } else {
                checker
                    .merge_array_element_type(elem_ty, &val_ty)
                    .unwrap_or(PhpType::Mixed)
            };
            env.insert(
                array.to_string(),
                PhpType::AssocArray {
                    key: Box::new(merged_key),
                    value: Box::new(merged_value),
                },
            );
        } else if **elem_ty != val_ty {
            let merged_ty = checker
                .merge_array_element_type(elem_ty, &val_ty)
                .unwrap_or(PhpType::Mixed);
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
    } else if let PhpType::Object(class_name) = &arr_ty {
        if !checker.object_type_implements_interface(class_name, "ArrayAccess") {
            return Err(CompileError::new(
                span,
                "Object array assignment requires ArrayAccess",
            ));
        }
    }
    Ok(())
}

/// Validates a nested array assignment like `$arr[$i] = $value` where the target itself is an array access.
///
/// Type-checks the array, index, and value expressions, then validates that the array type supports
/// nested offset assignment. Allows `Mixed` and objects implementing `ArrayAccess`; rejects strings
/// and plain arrays.
///
/// Errors:
/// - Target is not an array access expression
/// - Target is a string (string offset assignment not supported)
/// - Target type does not support nested assignment (not `Mixed` or `ArrayAccess`)
pub(super) fn check_nested_array_assign(
    checker: &mut Checker,
    target: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        return Err(CompileError::new(span, "Invalid assignment target"));
    };

    let arr_ty = checker.infer_type_with_assignment_effects(array, env)?;
    checker.infer_type_with_assignment_effects(index, env)?;
    checker.infer_type_with_assignment_effects(value, env)?;
    match arr_ty {
        PhpType::Mixed => Ok(()),
        PhpType::Str => Err(CompileError::new(
            span,
            "String offset assignment is not supported",
        )),
        PhpType::Object(class_name)
            if checker.object_type_implements_interface(&class_name, "ArrayAccess") =>
        {
            Ok(())
        }
        _ => Err(CompileError::new(
            span,
            "Nested array assignment requires a Mixed or ArrayAccess target",
        )),
    }
}

/// Validates and updates the type environment for `$array[] = $value` (push) assignments.
///
/// Type-checks the value, then merges it into the element type of the array.
/// For `PhpType::Array`, updates the element type in `env` to the merged type.
/// For `PhpType::AssocArray`, merges the pushed value type and adds integer keys.
/// For buffers, returns an error (buffers do not support push).
/// For objects implementing `ArrayAccess`, allows the push without element type merging.
///
/// Errors:
/// - Undefined variable
/// - Buffer push (buffers require `buffer_new<T>(len)` for allocation)
/// - Object push without `ArrayAccess`
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
    super::locals::update_callable_assignment_metadata(checker, array, value, &val_ty, env)?;
    if let PhpType::Array(elem_ty) = &arr_ty {
        if **elem_ty != val_ty {
            let merged_ty = checker
                .merge_array_element_type(elem_ty, &val_ty)
                .unwrap_or(PhpType::Mixed);
            env.insert(array.to_string(), PhpType::Array(Box::new(merged_ty)));
        }
    } else if let PhpType::AssocArray {
        key,
        value: existing_value,
    } = &arr_ty
    {
        let merged_key = merge_array_key_types(*key.clone(), PhpType::Int);
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
    } else if matches!(arr_ty, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            span,
            "buffer<T> does not support push; allocate with buffer_new<T>(len)",
        ));
    } else if let PhpType::Object(class_name) = &arr_ty {
        if !checker.object_type_implements_interface(class_name, "ArrayAccess") {
            return Err(CompileError::new(
                span,
                "Object array push requires ArrayAccess",
            ));
        }
    }
    Ok(())
}

/// Type-checks `$arr[$key] =& $source` (reference assignment into an array element).
///
/// M2/M3 scope: a single-level array-element target (`$base[$i] =& $v`) whose base local is any
/// array, or a two-level nested target (`$base[$i][$j] =& $v`) whose outermost base is a plain
/// `Variable` local. The source must be a scalar (`int`/`bool`) local. A reference entry is stored
/// as a boxed reference cell read back through the Mixed hash path, so the element value type must
/// be `Mixed`; the base is therefore promoted/widened to `AssocArray { key, Mixed }`. For nested
/// targets the base must hold inner arrays (also stored as `Mixed`), so its key and value are both
/// widened to `Mixed`. Anything outside this shape — three-or-more-level nesting, a non-`Variable`
/// outermost base, a non-array single-level base, property targets, or non-scalar/undefined sources
/// — returns the same "not yet supported" diagnostic the M0 gate produced, so unimplemented forms
/// fail cleanly instead of miscompiling.
pub(super) fn check_ref_assign_target(
    checker: &mut Checker,
    target: &Expr,
    source: &str,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let unsupported = || {
        CompileError::new(
            span,
            "Reference assignment into an array element or object property is not yet supported",
        )
    };
    // The source must be a scalar local for every supported target shape (array element or object
    // property): a reference entry stores the value through a boxed reference cell.
    let source_ty = env
        .get(source)
        .cloned()
        .ok_or_else(|| CompileError::new(span, &format!("Undefined variable: ${}", source)))?;
    if !matches!(
        source_ty,
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Mixed
    ) {
        return Err(unsupported());
    }
    match &target.kind {
        ExprKind::ArrayAccess { array, index } => match &array.kind {
            // Single-level target `$base[$index] =& $source`.
            ExprKind::Variable(array_name) => {
                check_single_level_ref_assign_target(checker, array_name, index, span, env)
            }
            // Two-level nested target `$base[$outer][$inner] =& $source` with a plain `Variable` base.
            ExprKind::ArrayAccess {
                array: inner_array,
                index: outer_index,
            } => {
                let ExprKind::Variable(array_name) = &inner_array.kind else {
                    return Err(unsupported());
                };
                check_nested_ref_assign_target(checker, array_name, outer_index, index, span, env)
            }
            _ => Err(unsupported()),
        },
        // Object dynamic-property target `$object->property =& $source` (stdClass hash path).
        ExprKind::PropertyAccess { object, .. } => {
            check_property_ref_assign_target(checker, object, span, env)
        }
        _ => Err(unsupported()),
    }
}

/// Validates the receiver for an object-property reference target `$object->property =& $source`.
///
/// Only a statically-typed `stdClass` instance is supported: its dynamic properties live in a hash
/// of boxed Mixed values that can hold a tag-11 reference entry, and the receiver is a raw object
/// pointer the codegen reads `obj+8` from directly. Declared typed-class properties are packed
/// fields (not hash entries), and a `Mixed` receiver is a boxed pointer the property-hash codegen
/// cannot dereference, so both return the "not yet supported" diagnostic. The object expression is
/// inferred for its assignment effects; no environment widening is needed because dynamic properties
/// are already typed `mixed`.
fn check_property_ref_assign_target(
    checker: &mut Checker,
    object: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let unsupported = || {
        CompileError::new(
            span,
            "Reference assignment into an array element or object property is not yet supported",
        )
    };
    let object_ty = checker.infer_type_with_assignment_effects(object, env)?;
    match &object_ty {
        PhpType::Object(class_name) if class_name.trim_start_matches('\\') == "stdClass" => Ok(()),
        _ => Err(unsupported()),
    }
}

/// Validates and widens the base for a single-level reference target `$base[$index] =& $source`.
///
/// The base local must already be an array (empty, indexed, or associative); it is widened in `env`
/// to `AssocArray { merged_key, Mixed }`, merging the prior key type with the (normalized) index
/// type. A non-array base returns the "not yet supported" diagnostic.
fn check_single_level_ref_assign_target(
    checker: &mut Checker,
    array_name: &str,
    index: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let unsupported = || {
        CompileError::new(
            span,
            "Reference assignment into an array element or object property is not yet supported",
        )
    };
    let array_ty = env
        .get(array_name)
        .cloned()
        .ok_or_else(|| CompileError::new(span, &format!("Undefined variable: ${}", array_name)))?;
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let normalized_idx = normalized_array_key_type(index, idx_ty);
    // A reference entry always forces Mixed value storage (the cell deref yields a boxed Mixed),
    // so widen the base to an associative array of Mixed regardless of its prior element type.
    let merged_key = match &array_ty {
        PhpType::Array(elem_ty) => {
            if matches!(elem_ty.as_ref(), PhpType::Never) {
                normalized_idx
            } else {
                merge_array_key_types(PhpType::Int, normalized_idx)
            }
        }
        PhpType::AssocArray { key, .. } => merge_array_key_types(*key.clone(), normalized_idx),
        _ => return Err(unsupported()),
    };
    env.insert(
        array_name.to_string(),
        PhpType::AssocArray {
            key: Box::new(merged_key),
            value: Box::new(PhpType::Mixed),
        },
    );
    Ok(())
}

/// Validates and widens the base for a nested reference target `$base[$outer][$inner] =& $source`.
///
/// The base local must already be an array; because its elements now hold inner arrays (stored as
/// boxed `Mixed`) and the inner element holds a reference cell (also `Mixed`), the base is widened
/// to `AssocArray { Mixed, Mixed }`. Both index expressions are inferred for their assignment
/// effects. A non-array base returns the "not yet supported" diagnostic.
fn check_nested_ref_assign_target(
    checker: &mut Checker,
    array_name: &str,
    outer_index: &Expr,
    inner_index: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let unsupported = || {
        CompileError::new(
            span,
            "Reference assignment into an array element or object property is not yet supported",
        )
    };
    let array_ty = env
        .get(array_name)
        .cloned()
        .ok_or_else(|| CompileError::new(span, &format!("Undefined variable: ${}", array_name)))?;
    if !matches!(array_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(unsupported());
    }
    // Infer both index expressions so their assignment effects are recorded, then widen the base to
    // a fully-Mixed associative array (its values are inner arrays, themselves stored as Mixed).
    checker.infer_type_with_assignment_effects(outer_index, env)?;
    checker.infer_type_with_assignment_effects(inner_index, env)?;
    env.insert(
        array_name.to_string(),
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
    Ok(())
}
