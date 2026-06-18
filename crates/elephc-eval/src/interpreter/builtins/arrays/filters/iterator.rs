//! Purpose:
//! Implements iterator-related array eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays::filters` re-exports.
//!
//! Key details:
//! - Iterator objects are driven through `rewind`, `valid`, callback, and `next`
//!   calls in PHP-observable order.

use super::super::super::super::*;
use super::super::super::*;
use super::*;

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
