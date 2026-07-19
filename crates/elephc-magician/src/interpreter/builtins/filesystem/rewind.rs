//! Purpose:
//! Declarative eval registry entry for `rewind`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "rewind",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `rewind` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rewind_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_rewind(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `rewind` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rewind_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_rewind_result(*stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `rewind($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_rewind(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_rewind_result(stream, context, values)
}

/// Rewinds a materialized stream resource to byte offset zero.
pub(in crate::interpreter) fn eval_rewind_result(
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    if let Some(seek_ok) = eval_user_wrapper_fseek_result(id, 0, 0, context, values)? {
        return values.bool_value(seek_ok);
    }
    values.bool_value(context.stream_resources_mut().rewind(id))
}
