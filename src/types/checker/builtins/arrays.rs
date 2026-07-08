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
        _ => Ok(None),
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
