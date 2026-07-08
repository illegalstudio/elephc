//! Purpose:
//! Declarative eval registry entry for `ftell`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "ftell",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `ftell` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_ftell_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_ftell(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `ftell` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_ftell_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_ftell_result(*stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `ftell($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ftell(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_ftell_result(stream, context, values)
}

/// Returns the current byte offset of a materialized stream resource.
pub(in crate::interpreter) fn eval_ftell_result(
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    if let Some(result) = eval_user_wrapper_ftell_result(id, context, values)? {
        return Ok(result);
    }
    match context.stream_resources_mut().tell(id) {
        Some(position) => values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}
