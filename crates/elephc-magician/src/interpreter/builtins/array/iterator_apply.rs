//! Purpose:
//! Implements eval iterator traversal and callback invocation for `iterator_apply`.
//! Argument arrays keep their owners through the complete traversal.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Callback arguments preserve PHP references until final writeback and cleanup.

use super::super::super::*;

eval_builtin! {
    contract: "iterator_apply",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `iterator_apply` array builtin.
pub(in crate::interpreter) fn eval_iterator_apply_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_iterator_apply(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `iterator_apply` array builtin.
pub(in crate::interpreter) fn eval_iterator_apply_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [iterator, callback] => {
            let callback = eval_callable(*callback, context, values)?;
            eval_iterator_apply_result(*iterator, &callback, Vec::new(), context, values)
        }
        [iterator, callback, args] => {
            let callback = eval_callable(*callback, context, values)?;
            with_iterator_apply_arguments(*args, context, values, |arguments, context, values| {
                eval_iterator_apply_result(*iterator, &callback, arguments, context, values)
            })
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
            let callback = eval_callable_from_scope(callback, context, scope, values)?;
            eval_iterator_apply_result(iterator, &callback, Vec::new(), context, values)
        }
        [iterator, callback, callback_args] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let callback = eval_callable_from_scope(callback, context, scope, values)?;
            let callback_args = eval_expr(callback_args, context, scope, values)?;
            with_iterator_apply_arguments(callback_args, context, values, |arguments, context, values| {
                eval_iterator_apply_result(iterator, &callback, arguments, context, values)
            })
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Keeps optional iterator callback arguments owned through the complete traversal.
fn with_iterator_apply_arguments<V: RuntimeValueOps>(
    args: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    invoke: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.is_null(args)? { return invoke(Vec::new(), context, values); }
    with_array_call_arguments(args, context, values, invoke)
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
