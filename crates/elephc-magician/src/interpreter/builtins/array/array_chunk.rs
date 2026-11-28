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
//! - Without `$preserve_keys` every chunk is a list, string keys included, and a `$length`
//!   below one is PHP's catchable `ValueError`, matching the compiled helper (#1295).

use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
use super::super::super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

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
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, length] => eval_array_chunk_result(*array, *length, false, context, values),
        [array, length, preserve_keys] => {
            let preserve_keys = values.truthy(*preserve_keys)?;
            eval_array_chunk_result(*array, *length, preserve_keys, context, values)
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
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |operands, context, _, values| {
        eval_array_chunk_declared_values_result(operands, context, values)
    })
}

/// Builds an `array_chunk()` result as nested reindexed or key-preserving chunks.
///
/// PHP renumbers every chunk from zero, string keys included, unless `$preserve_keys` is
/// truthy, in which case each chunk keeps the source keys of its own window. The outer array
/// is always a list. A `$length` below one raises PHP's catchable `ValueError` before any
/// chunk is built.
pub(in crate::interpreter) fn eval_array_chunk_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    preserve_keys: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let chunk_size = eval_int_value(length, values)?;
    if chunk_size <= 0 {
        return eval_throw_builtin_value_error(
            "array_chunk(): Argument #2 ($length) must be greater than 0",
            context,
            values,
        );
    }
    let chunk_size = usize::try_from(chunk_size).map_err(|_| EvalStatus::RuntimeFatal)?;
    let len = values.array_len(array)?;
    let chunk_count = len.div_ceil(chunk_size);
    let mut result = EvalArrayBuilder::indexed(values, chunk_count)?;
    for chunk_index in 0..chunk_count {
        let start = chunk_index * chunk_size;
        let end = usize::min(start + chunk_size, len);
        result.index(chunk_index, |values| {
            let mut chunk = if preserve_keys {
                EvalArrayBuilder::assoc(values, end - start)?
            } else {
                EvalArrayBuilder::indexed(values, end - start)?
            };
            for source_position in start..end {
                if preserve_keys {
                    chunk.entry_from_key(
                        |values| values.array_iter_key(array, source_position),
                        |values, key| values.array_get(array, key),
                    )?;
                } else {
                    chunk.index(source_position - start, |values| {
                        let key = RuntimeOwnerSlot::new(values.array_iter_key(array, source_position)?);
                        with_runtime_owners(&key, || {
                            let value = values.array_get(array, key.borrowed());
                            finish_eval_argument_values(value, [key.take()], context, values)
                        })
                    })?;
                }
            }
            Ok(chunk.finish())
        })?;
    }
    Ok(result.finish())
}
