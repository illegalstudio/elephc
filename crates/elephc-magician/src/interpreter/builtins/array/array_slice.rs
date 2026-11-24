//! Purpose:
//! Declarative eval registry entry for `array_slice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-slice hook.
//! - The parameter list mirrors PHP's
//!   `array_slice(array $array, int $offset, ?int $length = null, bool $preserve_keys = false)`
//!   and must stay shape-identical to the static registry declaration, which the builtin parity
//!   gate asserts.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::{EvalArrayBuilder, with_collection_cells};

eval_builtin! {
    contract: "array_slice",
    area: Array,
    direct: ArraySlice,
    values: ArraySlice,
}
/// Dispatches direct eval calls for the `array_slice` array builtin.
pub(in crate::interpreter) fn eval_array_slice_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_slice(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_slice` array builtin.
pub(in crate::interpreter) fn eval_array_slice_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, offset] => eval_array_slice_result(*array, *offset, None, false, values),
        [array, offset, length] => {
            eval_array_slice_result(*array, *offset, Some(*length), false, values)
        }
        [array, offset, length, preserve_keys] => {
            let preserve_keys = values.truthy(*preserve_keys)?;
            eval_array_slice_result(*array, *offset, Some(*length), preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_slice()` over array, offset, optional length, and preserve-keys expressions.
pub(in crate::interpreter) fn eval_builtin_array_slice(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array, offset] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_array_slice_result(array, offset, None, false, values)
        }
        [array, offset, length] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_array_slice_result(array, offset, Some(length), false, values)
        }
        [array, offset, length, preserve_keys] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_array_slice_result(array, offset, Some(length), preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_slice()` result with PHP offset, length, and key-preservation rules.
///
/// PHP renumbers the integer keys of the selected window from zero unless `$preserve_keys` is
/// truthy, while STRING keys are always preserved. The result therefore becomes an associative
/// container as soon as keys survive, exactly like `array_reverse()`'s key-preserving form.
pub(in crate::interpreter) fn eval_array_slice_result(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    preserve_keys: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let offset = eval_int_value(offset, values)?;
    let start = eval_slice_start(len, offset)?;
    let end = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            eval_slice_end(len, start, eval_int_value(length, values)?)?
        }
        _ => len,
    };

    let has_string_key = if preserve_keys {
        false
    } else {
        with_collection_cells(values, |values, owners| {
            for position in start..end {
                let key = owners.stage(values.array_iter_key(array, position)?);
                if values.type_tag(key)? == EVAL_TAG_STRING {
                    return Ok(true);
                }
            }
            Ok(false)
        })?
    };
    let mut result = if preserve_keys || has_string_key {
        EvalArrayBuilder::assoc(values, end.saturating_sub(start))?
    } else {
        EvalArrayBuilder::indexed(values, end.saturating_sub(start))?
    };
    let mut next_numeric_key = 0_i64;
    for position in start..end {
        result.update(|values, owners, _| {
            let key = owners.stage(values.array_iter_key(array, position)?);
            let value = owners.stage(values.array_get(array, key)?);
            let target_key = if preserve_keys || values.type_tag(key)? == EVAL_TAG_STRING {
                key
            } else {
                let key = owners.stage(values.int(next_numeric_key)?);
                next_numeric_key += 1;
                key
            };
            Ok(Some((target_key, value)))
        })?;
    }
    Ok(result.finish())
}

/// Converts a PHP array-slice offset into a bounded source position.
pub(in crate::interpreter) fn eval_slice_start(
    len: usize,
    offset: i64,
) -> Result<usize, EvalStatus> {
    if offset >= 0 {
        let offset = usize::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        return Ok(usize::min(offset, len));
    }

    let tail = offset
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    Ok(len.saturating_sub(tail))
}

/// Converts a PHP array-slice length into a bounded exclusive end position.
pub(in crate::interpreter) fn eval_slice_end(
    len: usize,
    start: usize,
    length: i64,
) -> Result<usize, EvalStatus> {
    if length >= 0 {
        let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
        return Ok(usize::min(start.saturating_add(length), len));
    }

    let tail = length
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    Ok(usize::max(start, len.saturating_sub(tail)))
}
