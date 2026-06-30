//! Purpose:
//! Implements PHP `array_filter()` eval support.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays::filters` re-exports.
//!
//! Key details:
//! - Callback argument shape follows the supported filter mode and null callbacks
//!   use PHP truthiness.

use super::super::super::super::*;
use super::super::super::*;

/// Evaluates PHP `array_filter()` for null and callable filtering modes.
pub(in crate::interpreter) fn eval_builtin_array_filter(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_filter_result_from_scope(array, None, None, Some(scope), context, values)
        }
        [array, callback] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            eval_array_filter_result_from_scope(
                array,
                Some(callback),
                None,
                Some(scope),
                context,
                values,
            )
        }
        [array, callback, mode] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_array_filter_result_from_scope(
                array,
                Some(callback),
                Some(mode),
                Some(scope),
                context,
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Filters eval array entries through PHP truthiness or a callable callback.
pub(in crate::interpreter) fn eval_array_filter_result(
    array: RuntimeCellHandle,
    callback: Option<RuntimeCellHandle>,
    mode: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_array_filter_result_from_scope(array, callback, mode, None, context, values)
}

/// Filters eval array entries with optional lexical scope for callback names.
fn eval_array_filter_result_from_scope(
    array: RuntimeCellHandle,
    callback: Option<RuntimeCellHandle>,
    mode: Option<RuntimeCellHandle>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = match callback {
        Some(callback) if !values.is_null(callback)? => {
            Some(eval_callable_with_optional_scope(
                callback,
                context,
                lexical_scope,
                values,
            )?)
        }
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
        let keep = if let Some(callback) = callback.as_ref() {
            let args = eval_array_filter_callback_args(mode, key, value)?;
            let result = eval_evaluated_callable_with_values(callback, args, context, values)?;
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
