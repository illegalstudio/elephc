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
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

/// Type-checks array builtin functions.
///
/// Dispatches on `name` to validate arity, argument types, and return type for each
/// supported array function (count, sort, rsort, shuffle, natsort, natcasesort, asort,
/// arsort, ksort, krsort, isset, array_multisort). Builtins migrated to the registry
/// (e.g. array_keys, array_values, array_flip, array_reverse, array_unique,
/// array_slice, array_pad, array_combine, array_chunk, array_column, array_is_list,
/// array_merge, array_merge_recursive, array_diff, array_intersect, array_diff_key,
/// array_intersect_key, array_diff_assoc, array_intersect_assoc, array_replace,
/// array_replace_recursive, in_array, array_sum, array_product, array_rand,
/// array_key_exists, array_key_first, array_key_last, array_search, array_fill_keys,
/// array_fill, range, array_pop, array_shift, array_push, array_unshift, array_splice)
/// are handled by their `src/builtins/array/` homes before this dispatcher runs.
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
                // `isset($obj->prop)` on an undeclared property dispatches to
                // `__isset`; the helper infers the receiver but skips the bare
                // property access that would otherwise reject the property.
                if checker
                    .isset_unset_property_magic_class(arg, "__isset", env)?
                    .is_some()
                {
                    continue;
                }
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "array_multisort" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_multisort() takes exactly 2 arguments",
                ));
            }
            let ty1 = checker.infer_type(&args[0], env)?;
            let ty2 = checker.infer_type(&args[1], env)?;
            if !matches!(ty1, PhpType::Array(_)) || !matches!(ty2, PhpType::Array(_)) {
                return Err(CompileError::new(
                    span,
                    "array_multisort() arguments must be indexed arrays",
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        _ => Ok(None),
    }
}

/// Provides the Union member is countable array helper used by the arrays module.
fn union_member_is_countable_array(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    )
}
