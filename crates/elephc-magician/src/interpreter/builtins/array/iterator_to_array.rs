//! Purpose:
//! Declarative eval registry entry for `iterator_to_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    contract: "iterator_to_array",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `iterator_to_array` array builtin.
pub(in crate::interpreter) fn eval_iterator_to_array_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_iterator_to_array(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `iterator_to_array` array builtin.
pub(in crate::interpreter) fn eval_iterator_to_array_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [iterator] => eval_iterator_to_array_result(*iterator, true, context, values),
        [iterator, preserve_keys] => {
            let preserve_keys = values.truthy(*preserve_keys)?;
            eval_iterator_to_array_result(*iterator, preserve_keys, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
            eval_iterator_to_array_result(iterator, true, context, values)
        }
        [iterator, preserve_keys] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_iterator_to_array_result(iterator, preserve_keys, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Copies eval-supported array iterator inputs into a PHP array result.
pub(in crate::interpreter) fn eval_iterator_to_array_result(
    iterator: RuntimeCellHandle,
    preserve_keys: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(iterator)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        // `iterator_to_array()` takes any Traversable, which is most of the point of it. The
        // array-literal spread already knows how to drain one -- a generator, an `Iterator`, an
        // `IteratorAggregate` -- so this reads the same entries rather than a second walker.
        let entries = eval_spread_source_entries(iterator, context, values)?;
        let mut result = values.assoc_new(entries.len())?;
        for (position, (key, value)) in entries.into_iter().enumerate() {
            if preserve_keys {
                result = values.array_set(result, key, value)?;
            } else {
                values.release(key)?;
                let key = values.int(position as i64)?;
                result = values.array_set(result, key, value)?;
            }
        }
        return Ok(result);
    }
    if preserve_keys {
        return eval_array_copy_preserve_keys(iterator, values);
    }
    super::array_values::eval_array_values_result(iterator, values)
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
