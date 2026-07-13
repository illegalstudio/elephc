//! Purpose:
//! Declarative eval registry entry for `array_chunk`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_chunk",
    area: Array,
    params: [array, length],
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
    let [array, length] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_chunk_result(*array, *length, values)
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
