//! Purpose:
//! Type-checks the arrays PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{array_key_type_from_value_type, PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

/// Type-checks array builtin functions.
///
/// Dispatches on `name` to validate arity, argument types, and return type for each
/// supported array function (count, array_pop, in_array, array_keys, array_values, sort,
/// rsort, shuffle, natsort, natcasesort, asort, arsort, ksort, krsort, isset, array_push,
/// array_reverse, array_unique, array_flip, array_shift, array_sum, array_product, array_rand,
/// array_key_exists, array_search, array_merge, array_diff, array_intersect, array_diff_key,
/// array_intersect_key, array_unshift, array_combine, array_fill_keys, array_pad, array_fill,
/// array_slice, array_splice, array_chunk, array_column, range).
///
/// Returns `Ok(Some(PhpType))` with the inferred return type, `Ok(None)` for unknown
/// builtins (deferred to caller), or `Err(CompileError)` on arity/type mismatch.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "count" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "count() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            match &ty {
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed => {
                    Ok(Some(PhpType::Int))
                }
                PhpType::Union(members) if members.iter().all(union_member_is_countable_array) => {
                    Ok(Some(PhpType::Int))
                }
                PhpType::Object(class_name) => {
                    if checker.class_implements_interface(class_name, "Countable") {
                        Ok(Some(PhpType::Int))
                    } else {
                        Err(CompileError::new(
                            span,
                            "count() object argument must implement Countable",
                        ))
                    }
                }
                _ => Err(CompileError::new(
                    span,
                    "count() argument must be array or Countable object",
                )),
            }
        }
        "array_pop" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "array_pop() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            match ty {
                PhpType::Array(elem_ty) => Ok(Some(*elem_ty)),
                PhpType::AssocArray { value, .. } => Ok(Some(*value)),
                _ => Err(CompileError::new(span, "array_pop() argument must be array")),
            }
        }
        "in_array" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "in_array() takes exactly 2 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            let arr_ty = checker.infer_type(&args[1], env)?;
            if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    "in_array() second argument must be array",
                ));
            }
            // PHP `in_array()` returns bool. The runtime result is 0/1 either way, but
            // the static type drives echo/var_dump: `echo false` is "" (not "0").
            Ok(Some(PhpType::Bool))
        }
        "array_keys" | "array_values" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            match (name, &ty) {
                ("array_keys", PhpType::Array(_)) => {
                    Ok(Some(PhpType::Array(Box::new(PhpType::Int))))
                }
                ("array_keys", PhpType::AssocArray { key, .. }) => {
                    Ok(Some(PhpType::Array(key.clone())))
                }
                ("array_values", PhpType::Array(elem_ty)) => {
                    Ok(Some(PhpType::Array(elem_ty.clone())))
                }
                ("array_values", PhpType::AssocArray { value, .. }) => {
                    Ok(Some(PhpType::Array(value.clone())))
                }
                _ => Err(CompileError::new(
                    span,
                    &format!("{}() argument must be array", name),
                )),
            }
        }
        "sort" | "rsort" | "shuffle" | "natsort" | "natcasesort" | "asort" | "arsort"
        | "ksort" | "krsort" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    &format!("{}() argument must be array", name),
                ));
            }
            Ok(Some(if name == "sort" || name == "rsort" {
                PhpType::Void
            } else {
                PhpType::Void
            }))
        }
        "isset" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "isset() takes at least 1 argument"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "array_push" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "array_push() takes exactly 2 arguments"));
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            let val_ty = checker.infer_type(&args[1], env)?;
            if let PhpType::Array(_) = arr_ty {
                let _ = val_ty;
            } else {
                return Err(CompileError::new(
                    span,
                    "array_push() first argument must be array",
                ));
            }
            Ok(Some(PhpType::Void))
        }
        "array_reverse" | "array_unique" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    &format!("{}() argument must be array", name),
                ));
            }
            Ok(Some(ty))
        }
        "array_flip" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "array_flip() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            match ty {
                PhpType::Array(elem_ty) => Ok(Some(PhpType::AssocArray {
                    key: Box::new(array_key_type_from_value_type(*elem_ty)),
                    value: Box::new(PhpType::Int),
                })),
                PhpType::AssocArray { key, value } => Ok(Some(PhpType::AssocArray {
                    key: Box::new(array_key_type_from_value_type(*value)),
                    value: key,
                })),
                _ => Err(CompileError::new(span, "array_flip() argument must be array")),
            }
        }
        "array_shift" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "array_shift() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            match ty {
                PhpType::Array(elem_ty) => Ok(Some(*elem_ty)),
                PhpType::AssocArray { value, .. } => Ok(Some(*value)),
                _ => Err(CompileError::new(span, "array_shift() argument must be array")),
            }
        }
        "array_sum" | "array_product" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            match ty {
                PhpType::Array(ref elem_ty) if **elem_ty == PhpType::Float => {
                    Ok(Some(PhpType::Float))
                }
                PhpType::Array(_) => Ok(Some(PhpType::Int)),
                PhpType::AssocArray { ref value, .. } if **value == PhpType::Float => {
                    Ok(Some(PhpType::Float))
                }
                PhpType::AssocArray { .. } => Ok(Some(PhpType::Int)),
                _ => Err(CompileError::new(
                    span,
                    &format!("{}() argument must be array", name),
                )),
            }
        }
        "array_rand" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "array_rand() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(span, "array_rand() argument must be array"));
            }
            Ok(Some(PhpType::Int))
        }
        "array_key_exists" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_key_exists() takes exactly 2 arguments",
                ));
            }
            checker.infer_type(&args[0], env)?;
            let arr_ty = checker.infer_type(&args[1], env)?;
            if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    "array_key_exists() second argument must be array",
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        "array_search" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_search() takes exactly 2 arguments",
                ));
            }
            checker.infer_type(&args[0], env)?;
            let arr_ty = checker.infer_type(&args[1], env)?;
            if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    "array_search() second argument must be array",
                ));
            }
            match arr_ty {
                PhpType::AssocArray { key, .. } => {
                    Ok(Some(checker.normalize_union_type(vec![*key, PhpType::Bool])))
                }
                _ => Ok(Some(PhpType::Union(vec![PhpType::Int, PhpType::Bool]))),
            }
        }
        "array_merge" | "array_diff" | "array_intersect" | "array_diff_key"
        | "array_intersect_key" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            let ty1 = checker.infer_type(&args[0], env)?;
            let ty2 = checker.infer_type(&args[1], env)?;
            if !matches!(ty1, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    &format!("{}() first argument must be array", name),
                ));
            }
            if name == "array_merge" {
                Ok(Some(array_merge_return_type(ty1, ty2)))
            } else {
                Ok(Some(ty1))
            }
        }
        "array_unshift" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_unshift() takes exactly 2 arguments",
                ));
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    "array_unshift() first argument must be array",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "array_combine" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_combine() takes exactly 2 arguments",
                ));
            }
            let keys_ty = checker.infer_type(&args[0], env)?;
            let vals_ty = checker.infer_type(&args[1], env)?;
            let key_elem = match keys_ty {
                PhpType::Array(elem) => *elem,
                _ => {
                    return Err(CompileError::new(
                        span,
                        "array_combine() first argument must be array",
                    ));
                }
            };
            let val_elem = match vals_ty {
                PhpType::Array(elem) => *elem,
                _ => {
                    return Err(CompileError::new(
                        span,
                        "array_combine() second argument must be array",
                    ));
                }
            };
            Ok(Some(PhpType::AssocArray {
                key: Box::new(array_key_type_from_value_type(key_elem)),
                value: Box::new(val_elem),
            }))
        }
        "array_fill_keys" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_fill_keys() takes exactly 2 arguments",
                ));
            }
            let keys_ty = checker.infer_type(&args[0], env)?;
            let val_ty = checker.infer_type(&args[1], env)?;
            let key_elem = match keys_ty {
                PhpType::Array(elem) => *elem,
                _ => {
                    return Err(CompileError::new(
                        span,
                        "array_fill_keys() first argument must be array",
                    ));
                }
            };
            Ok(Some(PhpType::AssocArray {
                key: Box::new(array_key_type_from_value_type(key_elem)),
                value: Box::new(val_ty),
            }))
        }
        "array_pad" => {
            if args.len() != 3 {
                return Err(CompileError::new(span, "array_pad() takes exactly 3 arguments"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            checker.infer_type(&args[2], env)?;
            if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    "array_pad() first argument must be array",
                ));
            }
            Ok(Some(ty))
        }
        "array_fill" => {
            if args.len() != 3 {
                return Err(CompileError::new(span, "array_fill() takes exactly 3 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            let val_ty = checker.infer_type(&args[2], env)?;
            // A non-literal-zero start (keys start..start+count-1) builds a keyed Mixed-valued
            // hash; a literal-zero start builds the indexed path (str for string values, scalar
            // otherwise). Both branches must match the codegen emitter and infer_local_type.
            let start_is_literal_zero = matches!(args[0].kind, ExprKind::IntLiteral(0));
            if !start_is_literal_zero {
                Ok(Some(PhpType::AssocArray {
                    key: Box::new(PhpType::Int),
                    value: Box::new(PhpType::Mixed),
                }))
            } else {
                Ok(Some(PhpType::Array(Box::new(val_ty))))
            }
        }
        "array_slice" | "array_splice" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 2 or 3 arguments", name),
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            for arg in &args[1..] {
                checker.infer_type(arg, env)?;
            }
            if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                return Ok(Some(PhpType::Mixed));
            }
            if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    &format!("{}() first argument must be array", name),
                ));
            }
            Ok(Some(ty))
        }
        "array_chunk" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_chunk() takes exactly 2 arguments",
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            match ty {
                PhpType::Array(elem_ty) => {
                    Ok(Some(PhpType::Array(Box::new(PhpType::Array(elem_ty)))))
                }
                PhpType::AssocArray { .. } => Err(CompileError::new(
                    span,
                    "array_chunk() argument must be indexed array",
                )),
                _ => Err(CompileError::new(
                    span,
                    "array_chunk() first argument must be array",
                )),
            }
        }
        "array_column" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_column() takes exactly 2 arguments",
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            match ty {
                PhpType::Array(inner) => match *inner {
                    PhpType::AssocArray { value, .. } => Ok(Some(PhpType::Array(value))),
                    _ => Err(CompileError::new(
                        span,
                        "array_column() requires an array of associative arrays",
                    )),
                },
                _ => Err(CompileError::new(
                    span,
                    "array_column() first argument must be array",
                )),
            }
        }
        "range" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "range() takes exactly 2 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Array(Box::new(PhpType::Int))))
        }
        _ => Ok(None),
    }
}

/// Infers the static return type for `array_merge()`.
///
/// Empty indexed arrays carry `Array<Void>` in the checker; when the left operand is empty,
/// the merged result should use the right operand's element shape instead of preserving
/// the left operand's void element type.
fn array_merge_return_type(first: PhpType, second: PhpType) -> PhpType {
    match first {
        PhpType::Array(elem) if is_empty_array_element_type(elem.as_ref()) => match second {
            PhpType::Array(right) if is_scalar_merge_element_type(right.as_ref()) => {
                PhpType::Array(right)
            }
            _ => PhpType::Array(elem),
        },
        other => other,
    }
}

/// Returns true for the element sentinel used by statically empty indexed arrays.
fn is_empty_array_element_type(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Void)
}

/// Returns true for element types copied safely by the scalar merge runtime helper.
fn is_scalar_merge_element_type(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable | PhpType::Void
    )
}

/// Provides the Union member is countable array helper used by the arrays module.
fn union_member_is_countable_array(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    )
}
