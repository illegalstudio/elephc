//! Purpose:
//! Array key lookup, search, random, range, merge, explode, and implode helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Array cells remain opaque runtime handles and are manipulated through
//!   `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP `array_key_exists()` over a key and array expression.
pub(in crate::interpreter) fn eval_builtin_array_key_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [key, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let key = eval_expr(key, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    values.array_key_exists(key, array)
}

/// Evaluates PHP array search builtins over a needle and haystack expression.
pub(in crate::interpreter) fn eval_builtin_array_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [needle, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let needle = eval_expr(needle, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_array_search_result(name, needle, array, values)
}

/// Searches an eval array with PHP's default loose comparison semantics.
pub(in crate::interpreter) fn eval_array_search_result(
    name: &str,
    needle: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let equal = values.compare(EvalBinOp::LooseEq, needle, value)?;
        if values.truthy(equal)? {
            return match name {
                "in_array" => values.bool_value(true),
                "array_search" => Ok(key),
                _ => Err(EvalStatus::UnsupportedConstruct),
            };
        }
    }
    match name {
        "in_array" => values.bool_value(false),
        "array_search" => values.bool_value(false),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP value-set array builtins over two eval array expressions.
pub(in crate::interpreter) fn eval_builtin_array_value_set(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_value_set_result(name, left, right, values)
}

/// Builds `array_diff()` or `array_intersect()` using PHP's default string comparison mode.
pub(in crate::interpreter) fn eval_array_value_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let mut right_values = Vec::with_capacity(right_len);
    for position in 0..right_len {
        let key = values.array_iter_key(right, position)?;
        let value = values.array_get(right, key)?;
        right_values.push(values.string_bytes(value)?);
    }

    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let comparable = values.string_bytes(value)?;
        let found = right_values
            .iter()
            .any(|right_value| right_value == &comparable);
        let keep = match name {
            "array_diff" => !found,
            "array_intersect" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP key-set array builtins over two eval array expressions.
pub(in crate::interpreter) fn eval_builtin_array_key_set(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_key_set_result(name, left, right, values)
}

/// Builds `array_diff_key()` or `array_intersect_key()` by testing first-array keys.
pub(in crate::interpreter) fn eval_array_key_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let exists = values.array_key_exists(key, right)?;
        let found = values.truthy(exists)?;
        let keep = match name {
            "array_diff_key" => !found,
            "array_intersect_key" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP `array_rand()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_rand_result(array, values)
}

/// Returns a valid random key from a non-empty eval array.
pub(in crate::interpreter) fn eval_array_rand_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    if len == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let position = eval_random_position(len);
    values.array_iter_key(array, position)
}

/// Chooses a pseudo-random array position within `[0, len)`.
pub(in crate::interpreter) fn eval_random_position(len: usize) -> usize {
    (eval_random_u128() % (len as u128)) as usize
}

/// Produces a process-local pseudo-random word for non-cryptographic eval builtins.
pub(in crate::interpreter) fn eval_random_u128() -> u128 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = u128::from(EVAL_RANDOM_COUNTER.fetch_add(1, Ordering::Relaxed));
    let pid = u128::from(std::process::id());
    let mut value = nanos ^ (counter.wrapping_mul(0x9e37_79b9_7f4a_7c15)) ^ (pid << 64);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Evaluates PHP `rand()` and `mt_rand()` over zero args or an inclusive range.
pub(in crate::interpreter) fn eval_builtin_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_rand_result(None, None, values),
        [min, max] => {
            let min = eval_expr(min, context, scope, values)?;
            let max = eval_expr(max, context, scope, values)?;
            eval_rand_result(Some(min), Some(max), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `random_int()` over an inclusive integer range.
pub(in crate::interpreter) fn eval_builtin_random_int(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [min, max] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let min = eval_expr(min, context, scope, values)?;
    let max = eval_expr(max, context, scope, values)?;
    eval_random_int_result(min, max, values)
}

/// Returns one non-cryptographic random integer using PHP's inclusive range rules.
pub(in crate::interpreter) fn eval_rand_result(
    min: Option<RuntimeCellHandle>,
    max: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (min, max) = match (min, max) {
        (None, None) => (0, i64::from(i32::MAX)),
        (Some(min), Some(max)) => (eval_int_value(min, values)?, eval_int_value(max, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let low = min.min(max);
    let high = min.max(max);
    let width = (i128::from(high) - i128::from(low) + 1) as u128;
    let offset = (eval_random_u128() % width) as i128;
    let sampled = i128::from(low) + offset;
    let sampled = i64::try_from(sampled).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(sampled)
}

/// Returns one eval `random_int()` value in the inclusive range `[min, max]`.
pub(in crate::interpreter) fn eval_random_int_result(
    min: RuntimeCellHandle,
    max: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let min = eval_int_value(min, values)?;
    let max = eval_int_value(max, values)?;
    if min > max {
        return Err(EvalStatus::RuntimeFatal);
    }
    let width = (i128::from(max) - i128::from(min) + 1) as u128;
    let offset = (eval_random_u128() % width) as i128;
    let sampled = i128::from(min) + offset;
    let sampled = i64::try_from(sampled).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(sampled)
}

/// Evaluates PHP `range()` over integer-compatible start and end expressions.
pub(in crate::interpreter) fn eval_builtin_range(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, end] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let start = eval_expr(start, context, scope, values)?;
    let end = eval_expr(end, context, scope, values)?;
    eval_range_result(start, end, values)
}

/// Builds an inclusive ascending or descending integer `range()` result.
pub(in crate::interpreter) fn eval_range_result(
    start: RuntimeCellHandle,
    end: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let end = eval_int_value(end, values)?;
    let distance = if start <= end {
        end.checked_sub(start).ok_or(EvalStatus::RuntimeFatal)?
    } else {
        start.checked_sub(end).ok_or(EvalStatus::RuntimeFatal)?
    };
    let count = distance.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let step = if start <= end { 1_i64 } else { -1_i64 };
    let mut current = start;
    let mut result = values.array_new(count)?;

    for index in 0..count {
        let key = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        let value = values.int(current)?;
        result = values.array_set(result, key, value)?;
        if index + 1 < count {
            current = current.checked_add(step).ok_or(EvalStatus::RuntimeFatal)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP `array_merge()` over two array expressions.
pub(in crate::interpreter) fn eval_builtin_array_merge(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_merge_result(left, right, values)
}

/// Builds an `array_merge()` result with PHP numeric reindexing and string-key overwrites.
pub(in crate::interpreter) fn eval_array_merge_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let capacity = left_len
        .checked_add(right_len)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut result = values.assoc_new(capacity)?;
    let mut next_numeric_key = 0_i64;
    result = eval_array_merge_append_operand(result, left, &mut next_numeric_key, values)?;
    eval_array_merge_append_operand(result, right, &mut next_numeric_key, values)
}

/// Appends one source array to an `array_merge()` result using PHP key handling.
pub(in crate::interpreter) fn eval_array_merge_append_operand(
    mut result: RuntimeCellHandle,
    source: RuntimeCellHandle,
    next_numeric_key: &mut i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(source)?;
    for position in 0..len {
        let source_key = values.array_iter_key(source, position)?;
        let source_value = values.array_get(source, source_key)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_STRING {
            source_key
        } else {
            let target_key = values.int(*next_numeric_key)?;
            *next_numeric_key = (*next_numeric_key)
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            target_key
        };
        result = values.array_set(result, target_key, source_value)?;
    }
    Ok(result)
}

/// Evaluates PHP `explode()` over separator and string expressions.
pub(in crate::interpreter) fn eval_builtin_explode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, string] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let string = eval_expr(string, context, scope, values)?;
    eval_explode_result(separator, string, values)
}

/// Splits one PHP byte string into an indexed array using a non-empty separator.
pub(in crate::interpreter) fn eval_explode_result(
    separator: RuntimeCellHandle,
    string: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let separator = values.string_bytes(separator)?;
    if separator.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let string = values.string_bytes(string)?;
    let mut result = values.array_new(0)?;
    let mut start = 0;
    let mut index = 0_i64;
    while let Some(found) = eval_find_subslice(&string, &separator, start) {
        result = eval_push_explode_segment(result, index, &string[start..found], values)?;
        start = found + separator.len();
        index += 1;
    }
    eval_push_explode_segment(result, index, &string[start..], values)
}

/// Appends one split segment to an indexed `explode()` result array.
pub(in crate::interpreter) fn eval_push_explode_segment(
    array: RuntimeCellHandle,
    index: i64,
    segment: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(index)?;
    let value = values.string_bytes_value(segment)?;
    values.array_set(array, key, value)
}

/// Finds `needle` inside `haystack` starting from one byte offset.
pub(in crate::interpreter) fn eval_find_subslice(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
) -> Option<usize> {
    haystack
        .get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + start)
}

/// Evaluates PHP `implode()` over separator and array expressions.
pub(in crate::interpreter) fn eval_builtin_implode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_implode_result(separator, array, values)
}

/// Joins array values in eval iteration order using PHP string conversion.
pub(in crate::interpreter) fn eval_implode_result(
    separator: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let separator = values.string_bytes(separator)?;
    let len = values.array_len(array)?;
    let mut output = Vec::new();
    for position in 0..len {
        if position > 0 {
            output.extend_from_slice(&separator);
        }
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let value = values.string_bytes(value)?;
        output.extend_from_slice(&value);
    }
    values.string_bytes_value(&output)
}
