//! Purpose:
//! Declarative eval registry entry for `fgets`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    contract: "fgets",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fgets` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fgets_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fgets(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fgets` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fgets_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_fgets_result(*stream, None, context, values),
        [stream, length] => {
            let bound = eval_fgets_length_bound(*length, values)?;
            eval_fgets_result(*stream, bound, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fgets($stream, $length?)` over its eval expressions.
pub(in crate::interpreter) fn eval_builtin_fgets(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (stream, length) = match args {
        [stream] => (stream, None),
        [stream, length] => (stream, Some(length)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let bound = match length {
        None => None,
        Some(length) => {
            let length = eval_expr(length, context, scope, values)?;
            eval_fgets_length_bound(length, values)?
        }
    };
    eval_fgets_result(stream, bound, context, values)
}

/// Converts PHP's `$length` into the byte bound `read_line` takes.
///
/// PHP reads at most `$length - 1` bytes. A non-positive `$length` is a `ValueError` there;
/// the eval interpreter reports it as a runtime fatal, which is how it surfaces other
/// argument-value rejections.
fn eval_fgets_length_bound(
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let length = eval_int_value(length, values)?;
    if length < 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(Some((length - 1) as usize))
}

/// Reads one newline-terminated string from a materialized stream resource.
pub(in crate::interpreter) fn eval_fgets_result(
    stream: RuntimeCellHandle,
    bound: Option<usize>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    if let Some(result) = eval_user_wrapper_fgets_result(id, context, values)? {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .read_line(id, bound.unwrap_or(usize::MAX), None, true, true)
    {
        Some(bytes) if !bytes.is_empty() => values.string_bytes_value(&bytes),
        Some(_) => values.bool_value(false),
        None => values.bool_value(false),
    }
}
