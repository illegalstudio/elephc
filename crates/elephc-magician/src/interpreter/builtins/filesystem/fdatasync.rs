//! Purpose:
//! Declarative eval registry entry for `fdatasync`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "fdatasync",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fdatasync` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fdatasync_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::fsync::eval_builtin_stream_sync("fdatasync", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fdatasync` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fdatasync_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => super::fsync::eval_stream_sync_result("fdatasync", *stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
