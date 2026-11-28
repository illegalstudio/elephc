//! Purpose:
//! Declarative eval registry entry for `array_search`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-search hook.
//! - `$strict` selects `===` over the default `==`, exactly as PHP's
//!   `array_search(mixed $needle, array $haystack, bool $strict = false)` does; `in_array()`
//!   shares the same scan.

use super::super::super::*;
use crate::runtime_hooks::owner_frames::with_runtime_owners;

eval_builtin! {
    contract: "array_search",
    area: Array,
    direct: ArraySearch,
    values: ArraySearch,
}
/// Dispatches direct eval calls for the `array_search` array builtin.
pub(in crate::interpreter) fn eval_array_search_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_search("array_search", args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_search` array builtin.
pub(in crate::interpreter) fn eval_array_search_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (needle, array, strict) = eval_array_search_value_args(evaluated_args, values)?;
    eval_array_search_result("array_search", needle, array, strict, context, values)
}

/// Splits evaluated `in_array()`/`array_search()` arguments into needle, haystack, and `$strict`.
///
/// A missing third argument is PHP's `false` default; any other arity is refused.
pub(in crate::interpreter) fn eval_array_search_value_args(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle, bool), EvalStatus> {
    match evaluated_args {
        [needle, array] => Ok((*needle, *array, false)),
        [needle, array, strict] => Ok((*needle, *array, values.truthy(*strict)?)),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP array search builtins over needle, haystack, and optional `$strict` expressions.
pub(in crate::interpreter) fn eval_builtin_array_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (needle, array, strict) = match args {
        [needle, array] => (needle, array, None),
        [needle, array, strict] => (needle, array, Some(strict)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let needle = eval_expr(needle, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    let strict = match strict {
        Some(strict) => {
            let strict = eval_expr(strict, context, scope, values)?;
            values.truthy(strict)?
        }
        None => false,
    };
    eval_array_search_result(name, needle, array, strict, context, values)
}

/// Searches an eval array with PHP's loose `==` comparison, or `===` when `strict` is set.
pub(in crate::interpreter) fn eval_array_search_result(
    name: &str,
    needle: RuntimeCellHandle,
    array: RuntimeCellHandle,
    strict: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let comparison = if strict { EvalBinOp::StrictEq } else { EvalBinOp::LooseEq };
    let len = values.array_len(array)?;
    for position in 0..len {
        let mut owners = Vec::with_capacity(3);
        let mut found = false;
        let result = (|| {
            let key = values.array_iter_key(array, position)?;
            owners.push(key);
            let value = with_runtime_owners(&owners, || values.array_get(array, key))?;
            owners.push(value);
            let equal = with_runtime_owners(&owners, || values.compare(comparison, needle, value))?;
            owners.push(equal);
            found = values.truthy(equal)?;
            // The finisher promotes a borrowed result before retiring the scan's leases.
            Ok(if found && name == "array_search" { key.borrowed() } else { equal.borrowed() })
        })();
        let result = finish_eval_argument_values(result, owners, context, values)?;
        if found { return Ok(result); }
        eval_release_value(context, values, result)?;
    }
    match name {
        "in_array" => values.bool_value(false),
        "array_search" => values.bool_value(false),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
