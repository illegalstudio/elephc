//! Purpose:
//! Declarative eval registry entry for `array_chunk`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.
//! - The parameter list mirrors PHP's
//!   `array_chunk(array $array, int $length, bool $preserve_keys = false)` and must stay
//!   shape-identical to the static registry declaration, which the builtin parity gate asserts.

use super::super::super::*;

eval_builtin! {
    contract: "array_chunk",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_chunk` array builtin.
pub(in crate::interpreter) fn eval_array_chunk_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_chunk(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_chunk` array builtin.
pub(in crate::interpreter) fn eval_array_chunk_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, length] => eval_array_chunk_result(*array, *length, false, values),
        [array, length, preserve_keys] => {
            let preserve_keys = values.truthy(*preserve_keys)?;
            eval_array_chunk_result(*array, *length, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_chunk()` over array, chunk-size, and preserve-keys expressions.
pub(in crate::interpreter) fn eval_builtin_array_chunk(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array, length] => {
            let array = eval_expr(array, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            super::array_arg_check::eval_check_array_args(
                "array_chunk",
                &[array],
                context,
                values,
            )?;
            eval_array_chunk_result(array, length, false, values)
        }
        [array, length, preserve_keys] => {
            let array = eval_expr(array, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            super::array_arg_check::eval_check_array_args(
                "array_chunk",
                &[array],
                context,
                values,
            )?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_array_chunk_result(array, length, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_chunk()` result as nested reindexed or key-preserving chunks.
///
/// PHP renumbers every chunk from zero unless `$preserve_keys` is truthy, in which case each
/// chunk keeps the source keys of its own window. The outer array is always a list.
pub(in crate::interpreter) fn eval_array_chunk_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    preserve_keys: bool,
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
        let mut keys = Vec::with_capacity(end - start);
        let mut has_string_key = false;
        for source_position in start..end {
            let key = values.array_iter_key(array, source_position)?;
            has_string_key |= values.type_tag(key)? == EVAL_TAG_STRING;
            keys.push(key);
        }
        let mut chunk = if preserve_keys || has_string_key {
            values.assoc_new(end - start)?
        } else {
            values.array_new(end - start)?
        };
        let mut next_numeric_key = 0_i64;
        for key in keys {
            let value = values.array_get(array, key)?;
            let target_key = if preserve_keys || values.type_tag(key)? == EVAL_TAG_STRING {
                key
            } else {
                let target_key = values.int(next_numeric_key)?;
                next_numeric_key += 1;
                target_key
            };
            chunk = values.array_set(chunk, target_key, value)?;
        }
        let result_key = i64::try_from(chunk_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let result_key = values.int(result_key)?;
        result = values.array_set(result, result_key, chunk)?;
    }

    Ok(result)
}
