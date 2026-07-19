//! Purpose:
//! Declarative eval registry entry for `feof`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "feof",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `feof` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_feof_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_feof(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `feof` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_feof_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_feof_result(*stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `feof($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_feof(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_feof_result(stream, context, values)
}

/// Reports whether a materialized stream resource is at EOF.
pub(in crate::interpreter) fn eval_feof_result(
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    if let Some(result) = eval_user_wrapper_feof_result(id, context, values)? {
        return Ok(result);
    }
    values.bool_value(context.stream_resources().eof(id).unwrap_or(false))
}
