//! Purpose:
//! Array filter, chunk, slice, pad, projection, iterator, and reverse helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Array cells remain opaque runtime handles and are manipulated through
//!   `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP `array_filter()` for null and string-callback filtering modes.
pub(in crate::interpreter) fn eval_builtin_array_filter(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_filter_result(array, None, None, context, values)
        }
        [array, callback] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            eval_array_filter_result(array, Some(callback), None, context, values)
        }
        [array, callback, mode] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_array_filter_result(array, Some(callback), Some(mode), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Filters eval array entries through PHP truthiness or a string callback.
pub(in crate::interpreter) fn eval_array_filter_result(
    array: RuntimeCellHandle,
    callback: Option<RuntimeCellHandle>,
    mode: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = match callback {
        Some(callback) if !values.is_null(callback)? => Some(eval_callable_name(callback, values)?),
        _ => None,
    };
    let mode = match mode {
        Some(mode) => eval_array_filter_mode_value(mode, values)?,
        None => EVAL_ARRAY_FILTER_USE_VALUE,
    };

    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let keep = if let Some(callback) = callback.as_deref() {
            let args = eval_array_filter_callback_args(mode, key, value)?;
            let result = eval_callable_with_values(callback, args, context, values)?;
            values.truthy(result)?
        } else {
            values.truthy(value)?
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Reads and validates the optional `array_filter()` callback mode.
pub(in crate::interpreter) fn eval_array_filter_mode_value(
    mode: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let mode = eval_int_value(mode, values)?;
    match mode {
        EVAL_ARRAY_FILTER_USE_VALUE | EVAL_ARRAY_FILTER_USE_BOTH | EVAL_ARRAY_FILTER_USE_KEY => {
            Ok(mode)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds the callback argument list for one `array_filter()` entry.
pub(in crate::interpreter) fn eval_array_filter_callback_args(
    mode: i64,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    match mode {
        EVAL_ARRAY_FILTER_USE_VALUE => Ok(vec![value]),
        EVAL_ARRAY_FILTER_USE_BOTH => Ok(vec![value, key]),
        EVAL_ARRAY_FILTER_USE_KEY => Ok(vec![key]),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_chunk()` over one array and chunk-size expression.
pub(in crate::interpreter) fn eval_builtin_array_chunk(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_array_chunk_result(array, length, values)
}

/// Builds an `array_chunk()` result as nested reindexed arrays.
pub(in crate::interpreter) fn eval_array_chunk_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let chunk_size = eval_int_value(length, values)?;
    if chunk_size <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let chunk_size = usize::try_from(chunk_size).map_err(|_| EvalStatus::RuntimeFatal)?;
    let len = values.array_len(array)?;
    let chunk_count = len.div_ceil(chunk_size);
    let mut result = values.array_new(chunk_count)?;

    for chunk_index in 0..chunk_count {
        let start = chunk_index * chunk_size;
        let end = usize::min(start + chunk_size, len);
        let mut chunk = values.array_new(end - start)?;
        for source_position in start..end {
            let source_key = values.array_iter_key(array, source_position)?;
            let value = values.array_get(array, source_key)?;
            let target_index =
                i64::try_from(source_position - start).map_err(|_| EvalStatus::RuntimeFatal)?;
            let target_index = values.int(target_index)?;
            chunk = values.array_set(chunk, target_index, value)?;
        }
        let result_key = i64::try_from(chunk_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let result_key = values.int(result_key)?;
        result = values.array_set(result, result_key, chunk)?;
    }

    Ok(result)
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

/// Evaluates PHP `array_pad()` over array, target length, and pad value expressions.
pub(in crate::interpreter) fn eval_builtin_array_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_pad_result(array, length, value, values)
}

/// Builds an `array_pad()` result by copying values and padding left or right.
pub(in crate::interpreter) fn eval_array_pad_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let target = eval_int_value(length, values)?;
    let target_len = target
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    let result_len = usize::max(len, target_len);
    let pad_count = result_len.saturating_sub(len);
    let mut result = values.array_new(result_len)?;
    let mut output_index = 0usize;

    if target < 0 {
        let (padded, next_index) =
            eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?;
        result = padded;
        output_index = next_index;
    }

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let source_value = values.array_get(array, source_key)?;
        let target_key = i64::try_from(output_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let target_key = values.int(target_key)?;
        result = values.array_set(result, target_key, source_value)?;
        output_index += 1;
    }

    if target > 0 {
        result = eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?.0;
    }

    Ok(result)
}

/// Appends the same pad value at consecutive indexed positions in an array result.
pub(in crate::interpreter) fn eval_array_pad_append_repeated(
    mut array: RuntimeCellHandle,
    start_index: usize,
    count: usize,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, usize), EvalStatus> {
    let mut next_index = start_index;
    for _ in 0..count {
        let key = i64::try_from(next_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        array = values.array_set(array, key, value)?;
        next_index += 1;
    }
    Ok((array, next_index))
}

/// Evaluates PHP `array_flip()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_flip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_flip_result(array, values)
}

/// Builds the associative result for `array_flip()` using PHP's valid value-key subset.
pub(in crate::interpreter) fn eval_array_flip_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        if !matches!(values.type_tag(value)?, EVAL_TAG_INT | EVAL_TAG_STRING) {
            continue;
        }
        result = values.array_set(result, value, key)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_unique()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_unique(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_unique_result(array, values)
}

/// Builds `array_unique()` by comparing values with PHP's default string comparison mode.
pub(in crate::interpreter) fn eval_array_unique_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut seen = Vec::<Vec<u8>>::with_capacity(len);
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let unique_key = values.string_bytes(value)?;
        if seen.iter().any(|seen_key| seen_key == &unique_key) {
            continue;
        }
        seen.push(unique_key);
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP array projection builtins over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_projection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_projection_result(name, array, values)
}

/// Builds the indexed result array for `array_keys()` or `array_values()`.
pub(in crate::interpreter) fn eval_array_projection_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = match name {
            "array_keys" => key,
            "array_values" => values.array_get(array, key)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        let index = values.int(position as i64)?;
        result = values.array_set(result, index, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `iterator_apply()` for eval-supported Traversable object inputs.
pub(in crate::interpreter) fn eval_builtin_iterator_apply(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [iterator, callback] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let callback = eval_callable(callback, values)?;
            eval_iterator_apply_result(iterator, &callback, Vec::new(), context, values)
        }
        [iterator, callback, callback_args] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let callback = eval_callable(callback, values)?;
            let callback_args = eval_expr(callback_args, context, scope, values)?;
            let callback_args = eval_iterator_apply_arg_values(callback_args, values)?;
            eval_iterator_apply_result(iterator, &callback, callback_args, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts the optional `iterator_apply()` callback-args value into call arguments.
pub(in crate::interpreter) fn eval_iterator_apply_arg_values(
    args: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    if values.is_null(args)? {
        return Ok(Vec::new());
    }
    if !values.is_array_like(args)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_array_call_arg_values(args, values)
}

/// Applies a callback to each valid position of an eval-supported Traversable object.
pub(in crate::interpreter) fn eval_iterator_apply_result(
    iterator: RuntimeCellHandle,
    callback: &EvaluatedCallable,
    callback_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(iterator)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let count = match eval_iterator_apply_iterator_object(
        iterator,
        callback,
        &callback_args,
        context,
        values,
    ) {
        Ok(count) => count,
        Err(EvalStatus::UnsupportedConstruct) => {
            let iterator = values.method_call(iterator, "getiterator", Vec::new())?;
            eval_iterator_apply_iterator_object(
                iterator,
                callback,
                &callback_args,
                context,
                values,
            )?
        }
        Err(err) => return Err(err),
    };
    values.int(count)
}

/// Drives one Iterator object through `rewind()`, `valid()`, callback, and `next()`.
pub(in crate::interpreter) fn eval_iterator_apply_iterator_object(
    iterator: RuntimeCellHandle,
    callback: &EvaluatedCallable,
    callback_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let _ = values.method_call(iterator, "rewind", Vec::new())?;
    let mut count = 0_i64;
    loop {
        let valid = values.method_call(iterator, "valid", Vec::new())?;
        if !values.truthy(valid)? {
            return Ok(count);
        }
        count = count.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        let result = eval_evaluated_callable_with_call_array_args(
            callback,
            callback_args.to_vec(),
            context,
            values,
        )?;
        if !values.truthy(result)? {
            return Ok(count);
        }
        let _ = values.method_call(iterator, "next", Vec::new())?;
    }
}

/// Evaluates PHP `iterator_count()` for eval-supported array iterator inputs.
pub(in crate::interpreter) fn eval_builtin_iterator_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [iterator] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let iterator = eval_expr(iterator, context, scope, values)?;
    eval_iterator_count_result(iterator, values)
}

/// Returns the element count for eval-supported array iterator inputs.
pub(in crate::interpreter) fn eval_iterator_count_result(
    iterator: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(iterator)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(iterator)?;
    values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `iterator_to_array()` for eval-supported array iterator inputs.
pub(in crate::interpreter) fn eval_builtin_iterator_to_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [iterator] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            eval_iterator_to_array_result(iterator, true, values)
        }
        [iterator, preserve_keys] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_iterator_to_array_result(iterator, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Copies eval-supported array iterator inputs into a PHP array result.
pub(in crate::interpreter) fn eval_iterator_to_array_result(
    iterator: RuntimeCellHandle,
    preserve_keys: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(iterator)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if preserve_keys {
        return eval_array_copy_preserve_keys(iterator, values);
    }
    eval_array_projection_result("array_values", iterator, values)
}

/// Copies one array-like eval value while preserving iteration keys and order.
pub(in crate::interpreter) fn eval_array_copy_preserve_keys(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_reverse()` over an eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_reverse(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_reverse_result(array, false, values)
        }
        [array, preserve_keys] => {
            let array = eval_expr(array, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_array_reverse_result(array, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_reverse()` result while preserving PHP key rules.
pub(in crate::interpreter) fn eval_array_reverse_result(
    array: RuntimeCellHandle,
    preserve_keys: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut keys = Vec::with_capacity(len);
    let mut has_string_key = false;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        has_string_key |= values.type_tag(key)? == EVAL_TAG_STRING;
        keys.push(key);
    }

    let mut result = if preserve_keys || has_string_key {
        values.assoc_new(len)?
    } else {
        values.array_new(len)?
    };
    let mut next_numeric_key = 0_i64;

    for key in keys.into_iter().rev() {
        let value = values.array_get(array, key)?;
        let target_key = if preserve_keys || values.type_tag(key)? == EVAL_TAG_STRING {
            key
        } else {
            let key = values.int(next_numeric_key)?;
            next_numeric_key += 1;
            key
        };
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}
