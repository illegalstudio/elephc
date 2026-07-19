//! Purpose:
//! Declarative eval registry entry for `fsync`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "fsync",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fsync` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fsync_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_sync("fsync", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fsync` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fsync_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_stream_sync_result("fsync", *stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fsync($stream)` or `fdatasync($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stream_sync(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_stream_sync_result(name, stream, context, values)
}

/// Synchronizes one materialized stream resource to storage.
pub(in crate::interpreter) fn eval_stream_sync_result(
    name: &str,
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let ok = match name {
        "fsync" => context.stream_resources_mut().sync_all(id),
        "fdatasync" => context.stream_resources_mut().sync_data(id),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}
