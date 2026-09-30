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
        // php evaluates EVERY argument before the type check throws (measured: a side
        // effect in `$offset` still runs when `$array` is `false`), so each arm checks
        // only once its whole operand list is in hand.
        [array, offset] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            super::array_arg_check::eval_check_array_args("array_slice", &[array], context, values)?;
            eval_array_slice_result(array, offset, None, false, values)
        }
        [array, offset, length] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            super::array_arg_check::eval_check_array_args("array_slice", &[array], context, values)?;
            eval_array_slice_result(array, offset, Some(length), false, values)
        }
        [array, offset, length, preserve_keys] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            super::array_arg_check::eval_check_array_args("array_slice", &[array], context, values)?;
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

    let mut keys = Vec::with_capacity(end.saturating_sub(start));
    let mut has_string_key = false;
    for source_position in start..end {
        let key = values.array_iter_key(array, source_position)?;
        has_string_key |= values.type_tag(key)? == EVAL_TAG_STRING;
        keys.push(key);
    }

    let mut result = if preserve_keys || has_string_key {
        values.assoc_new(end.saturating_sub(start))?
    } else {
        values.array_new(end.saturating_sub(start))?
    };
    let mut next_numeric_key = 0_i64;
    for key in keys {
        let source_value = values.array_get(array, key)?;
        let target_key = if preserve_keys || values.type_tag(key)? == EVAL_TAG_STRING {
            key
        } else {
            let target_key = values.int(next_numeric_key)?;
            next_numeric_key += 1;
            target_key
        };
        result = values.array_set(result, target_key, source_value)?;
    }
    Ok(result)
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
