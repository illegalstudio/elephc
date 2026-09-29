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
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::span::Span;
use crate::types::{
    merge_array_key_types, normalized_array_key_type, static_array_key_forces_hash_storage,
    PhpType, TypeEnv,
};

use super::super::super::Checker;

/// Validates and updates the type environment for `$array[$index] = $value` assignments.
///
/// Routes a string target to the string offset write rules, merges element types for
/// arrays/assoc-arrays, checks buffer index type and element type compatibility, and requires
/// ArrayAccess for objects. Updates `env` with the merged key/value types; returns an error for
/// invalid targets or type mismatches.
///
/// Errors:
/// - Undefined variable
/// - String offset write with a non-integer offset or a compound operator
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
        return check_string_offset_assign(array, index, &idx_ty, value, span);
    }
    if let PhpType::Array(elem_ty) = &arr_ty {
        let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
        // A foreach loop key is a boxed `Mixed` cell at runtime (`Op::IterCurrentKey`)
        // even when the checker types it as `Int`/`Str` from the source array, so it
        // may hold either an integer or a string and the destination must stay indexed
        // `Array(Mixed)` with the indexed-vs-hash decision deferred to the runtime
        // write helper (`Op::ArraySetMixedKey`). A non-foreach string-typed key (a
        // literal string, or a string-valued expression like `"k" . $i` or a plain
        // string variable) always means associative hash storage in PHP, so it
        // promotes to `AssocArray` and stays usable by direct string-key reads. A
        // non-foreach `Mixed`-typed key (e.g. a `mixed` parameter) is likewise a
        // runtime-tagged cell, so it stays `Array(Mixed)` to match the lowering's
        // `ArraySetMixedKey` routing.
        let index_is_foreach_key = matches!(&index.kind, ExprKind::Variable(name) if checker.is_foreach_key(name));
        let forces_hash = matches!(normalized_idx_ty, PhpType::Str)
            || (matches!(idx_ty, PhpType::Str) && !index_is_foreach_key)
            || (matches!(elem_ty.as_ref(), PhpType::Never)
                && static_array_key_forces_hash_storage(index));
        if forces_hash {
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
        } else if index_is_foreach_key || matches!(idx_ty, PhpType::Mixed) {
            env.insert(
                array.to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
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
        if !matches!(idx_ty, PhpType::Int | PhpType::Mixed) {
            return Err(CompileError::new(span, "Buffer index must be integer"));
        }
        match elem_ty.as_ref() {
            PhpType::Packed(_) => {
                return Err(CompileError::new(
                    span,
                    "Assign packed buffer elements through field access like $buf[$i]->field",
                ))
            }
            inner if !buffer_element_accepts_assignment(inner, &val_ty) => {
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

/// Diagnostic for a string offset write whose string lives in a property.
pub(super) const STRING_OFFSET_ON_PROPERTY_UNSUPPORTED: &str =
    "String offset assignment on a property is not supported; copy the property into a local \
     variable, assign the offset there, and store the local back";

/// Diagnostic for a string offset write whose string lives in an array element.
const STRING_OFFSET_ON_ELEMENT_UNSUPPORTED: &str =
    "String offset assignment on an array element is not supported; copy the element into a \
     local variable, assign the offset there, and store the local back";

/// Returns whether a nested write target's receiver is itself a string offset (`$s[0][0]`).
///
/// PHP refuses that write with `Error: Cannot use string offset as an array`, unlike a string
/// held in an array element (`$a["k"][0]`), which is a valid PHP write this compiler does not
/// lower yet.
fn receiver_is_string_offset(checker: &mut Checker, array: &Expr, env: &TypeEnv) -> bool {
    let ExprKind::ArrayAccess { array: receiver, .. } = &array.kind else {
        return false;
    };
    matches!(checker.infer_type(receiver, env), Ok(PhpType::Str))
}

/// Validates a string offset write `$s[$i] = $v` on a local typed `string`.
///
/// The offset must be one a string read also accepts (integer, float, `mixed`, or a numeric
/// string literal). PHP refuses a compound operator on a string offset
/// (`$s[0] .= "x"`) with a runtime `Error`; that shape always fails, so it is refused here
/// with PHP's wording. The local keeps its `string` type.
fn check_string_offset_assign(
    array: &str,
    index: &Expr,
    idx_ty: &PhpType,
    value: &Expr,
    span: Span,
) -> Result<(), CompileError> {
    if let Some(message) = string_offset_read_modify_write_error(array, index, value, span) {
        return Err(CompileError::new(span, message));
    }
    if !crate::types::checker::inference::is_valid_string_offset_index(index, idx_ty) {
        return Err(CompileError::new(span, "String index must be integer"));
    }
    Ok(())
}

/// Returns PHP's error for a desugared read-modify-write of a string offset, if `value` is one.
///
/// The parser rewrites `$s[$i] <op>= $v` and the statement form of `$s[$i]++` / `--$s[$i]`
/// into `$s[$i] = $s[$i] <op> <rhs>`, giving the synthesized value the statement's own span;
/// that separates it from a user-written `$s[0] = $s[0] . "x"`, which PHP allows. An
/// increment's synthesized `1` also carries the statement span, whereas the `1` of a written
/// `$s[0] += 1` carries its own. `??=` desugars to a null-coalesce and stays allowed, as in PHP.
fn string_offset_read_modify_write_error(
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) -> Option<&'static str> {
    if value.span != span {
        return None;
    }
    let ExprKind::BinaryOp { left, op, right } = &value.kind else {
        return None;
    };
    if !expr_is_string_offset_target(left, array, index) {
        return None;
    }
    let synthesized_step = right.span == span && matches!(right.kind, ExprKind::IntLiteral(1));
    if synthesized_step && matches!(op, BinOp::Add | BinOp::Sub) {
        Some("Cannot increment/decrement string offsets")
    } else {
        Some("Cannot use assign-op operators with string offsets")
    }
}

/// Returns whether `expr` is exactly the string offset target `$array[index]`.
pub(in crate::types::checker) fn expr_is_string_offset_target(
    expr: &Expr,
    array: &str,
    index: &Expr,
) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayAccess { array: receiver, index: read_index }
            if matches!(&receiver.kind, ExprKind::Variable(name) if name == array)
                && read_index.as_ref() == index
    )
}

/// Returns whether a buffer element accepts an assignment value after runtime coercion.
fn buffer_element_accepts_assignment(expected: &PhpType, actual: &PhpType) -> bool {
    if expected == actual {
        return true;
    }
    matches!(
        (expected, actual),
        (PhpType::Bool, PhpType::False)
            | (PhpType::Float | PhpType::Int | PhpType::Bool, PhpType::Mixed)
    )
}

/// Validates a nested array assignment like `$arr[$i] = $value` where the target itself is an array access.
///
/// Type-checks the array, index, and value expressions, then validates that the array type supports
/// nested offset assignment. Allows `Mixed` and objects implementing `ArrayAccess`; rejects strings
/// and plain arrays.
///
/// Errors:
/// - Target is not an array access expression
/// - Target is a string: `$s[0][0]` is PHP's "Cannot use string offset as an array", and a
///   string held in an array element is not lowered yet
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
        PhpType::Str if receiver_is_string_offset(checker, array, env) => Err(CompileError::new(
            span,
            "Cannot use string offset as an array",
        )),
        PhpType::Str => Err(CompileError::new(span, STRING_OFFSET_ON_ELEMENT_UNSUPPORTED)),
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
/// - String push (PHP's `[] operator not supported for strings`)
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
    if arr_ty == PhpType::Str {
        // PHP throws `Error` for `$s[] = $v` on a string before evaluating anything else.
        return Err(CompileError::new(span, "[] operator not supported for strings"));
    }
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
