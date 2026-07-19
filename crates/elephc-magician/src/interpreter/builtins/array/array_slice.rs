//! Purpose:
//! Declarative eval registry entry for `array_slice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-slice hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "array_slice",
    area: Array,
    params: [array, offset, length = EvalBuiltinDefaultValue::Null],
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
        [array, offset] => eval_array_slice_result(*array, *offset, None, values),
        [array, offset, length] => eval_array_slice_result(*array, *offset, Some(*length), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_slice()` over array, offset, and optional length expressions.
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
            eval_array_slice_result(array, offset, None, values)
        }
        [array, offset, length] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_array_slice_result(array, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_slice()` result with PHP offset and length bounds.
pub(in crate::interpreter) fn eval_array_slice_result(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
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

    let mut result = values.array_new(end.saturating_sub(start))?;
    for source_position in start..end {
        let source_key = values.array_iter_key(array, source_position)?;
        let source_value = values.array_get(array, source_key)?;
        let target_key =
            i64::try_from(source_position - start).map_err(|_| EvalStatus::RuntimeFatal)?;
        let target_key = values.int(target_key)?;
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
