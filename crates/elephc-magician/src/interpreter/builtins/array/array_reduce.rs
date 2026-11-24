//! Purpose:
//! Declarative eval registry entry for `array_reduce`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::{with_collection_cells, with_collection_result};

eval_builtin! {
    contract: "array_reduce",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_reduce` array builtin.
pub(in crate::interpreter) fn eval_array_reduce_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_reduce(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_reduce` array builtin.
pub(in crate::interpreter) fn eval_array_reduce_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, callback] => {
            let initial = None;
            eval_array_reduce_result_from_scope(
                *array,
                *callback,
                initial,
                lexical_scope,
                context,
                values,
            )
        }
        [array, callback, initial] => eval_array_reduce_result_from_scope(
            *array,
            *callback,
            Some(*initial),
            lexical_scope,
            context,
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_reduce()` with an optional initial carry value.
pub(in crate::interpreter) fn eval_builtin_array_reduce(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |args, context, scope, values| {
        eval_array_reduce_declared_values_result(args, Some(scope), context, values)
    })
}

/// Reduces one eval array with optional lexical scope for callback names.
fn eval_array_reduce_result_from_scope(
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    initial: Option<RuntimeCellHandle>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
    callback.with_values(context, values, |callback, context, values| {
        let len = values.array_len(array)?;
        with_collection_result(values, |values, carry| {
            let initial = match initial {
                Some(initial) => values.retain(initial)?,
                None => values.null()?,
            };
            carry.replace(Some(initial));
            for position in 0..len {
                with_collection_cells(values, |values, owners| {
                    let key = owners.stage(values.array_iter_key(array, position)?);
                    let value = owners.stage(values.array_get(array, key)?);
                    let next = eval_evaluated_callable_with_values(
                        &callback, vec![carry.borrowed(), value.borrowed()], context, values,
                    )?;
                    let next = promote_borrowed_result(next, values)?;
                    if let Some(previous) = carry.replace(Some(next)) {
                        owners.stage(previous);
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
    })
}
