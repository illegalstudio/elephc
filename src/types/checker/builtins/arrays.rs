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
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

/// Type-checks array builtin functions.
///
/// Dispatches on `name` to validate arity, argument types, and return type for each
/// supported array function (isset). Builtins migrated to the registry
/// (e.g. array_keys, array_values, array_flip, array_reverse, array_unique,
/// array_slice, array_pad, array_combine, array_chunk, array_column, array_is_list,
/// array_merge, array_merge_recursive, array_multisort, array_diff, array_intersect,
/// array_diff_key, array_intersect_key, array_diff_assoc, array_intersect_assoc,
/// array_replace, array_replace_recursive, in_array, array_sum, array_product,
/// array_rand, array_key_exists, array_key_first, array_key_last, array_search,
/// array_fill_keys, array_fill, range, array_pop, array_shift, array_push,
/// array_unshift, array_splice, sort, rsort, asort, arsort, ksort, krsort, natsort,
/// natcasesort, shuffle, count)
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
        "isset" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "isset() takes at least 1 argument"));
            }
            for arg in args {
                check_isset_arg(checker, arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        _ => Ok(None),
    }
}

/// Type-checks one `isset()` operand while preserving PHP's non-reading property semantics.
fn check_isset_arg(checker: &mut Checker, arg: &Expr, env: &TypeEnv) -> Result<(), CompileError> {
    if let ExprKind::PropertyAccess { object, .. }
    | ExprKind::NullsafePropertyAccess { object, .. } = &arg.kind
    {
        let object_ty = checker.infer_type(object, env)?;
        if isset_object_receiver_type(checker, &object_ty) {
            return Ok(());
        }
    }
    checker.infer_type(arg, env).map(|_| ())
}

/// Returns true when `isset($object->property)` can be checked without reading the property.
fn isset_object_receiver_type(checker: &Checker, ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::Mixed => true,
        PhpType::Union(members) => {
            checker.union_single_object_class(ty).is_some()
                || members.iter().any(|member| matches!(member, PhpType::Mixed))
        }
        _ => false,
    }
}

/// Returns `true` if a `PhpType` is a countable array type for Union membership checks.
///
/// Used by `crate::builtins::array::count` to test whether every branch of a Union type
/// is countable, in which case `count()` returns `Int` for the whole union.
pub(crate) fn union_member_is_countable_array(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    )
}
