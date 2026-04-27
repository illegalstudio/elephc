use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

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
            if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(span, "count() argument must be array"));
            }
            Ok(Some(PhpType::Int))
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
            Ok(Some(PhpType::Int))
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
            if args.len() != 1 {
                return Err(CompileError::new(span, "isset() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "array_push" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "array_push() takes exactly 2 arguments"));
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            let val_ty = checker.infer_type(&args[1], env)?;
            if let PhpType::Array(elem_ty) = arr_ty {
                if *elem_ty != val_ty {
                    return Err(CompileError::new(span, "array_push() type mismatch"));
                }
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
                    key: elem_ty,
                    value: Box::new(PhpType::Int),
                })),
                PhpType::AssocArray { key, value } => Ok(Some(PhpType::AssocArray {
                    key: value,
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
            if matches!(arr_ty, PhpType::AssocArray { .. }) {
                Ok(Some(PhpType::Union(vec![PhpType::Str, PhpType::Bool])))
            } else {
                Ok(Some(PhpType::Union(vec![PhpType::Int, PhpType::Bool])))
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
            checker.infer_type(&args[1], env)?;
            if !matches!(ty1, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                return Err(CompileError::new(
                    span,
                    &format!("{}() first argument must be array", name),
                ));
            }
            Ok(Some(ty1))
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
                key: Box::new(key_elem),
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
                key: Box::new(key_elem),
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
            Ok(Some(PhpType::Array(Box::new(val_ty))))
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
