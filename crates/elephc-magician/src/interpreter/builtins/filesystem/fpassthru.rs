//! Purpose:
//! Declarative eval registry entry for `fpassthru`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "fpassthru",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fpassthru` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fpassthru_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fpassthru(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fpassthru` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fpassthru_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_fpassthru_result(*stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fpassthru($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_fpassthru(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_fpassthru_result(stream, context, values)
}

/// Streams all remaining bytes to eval output and returns the emitted byte count.
pub(in crate::interpreter) fn eval_fpassthru_result(
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    if let Some(result) = eval_user_wrapper_fpassthru_result(id, context, values)? {
        return Ok(result);
    }
    let Some(bytes) = context.stream_resources_mut().get_contents(id, None, None) else {
        return values.bool_value(false);
    };
    let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(len)
}
