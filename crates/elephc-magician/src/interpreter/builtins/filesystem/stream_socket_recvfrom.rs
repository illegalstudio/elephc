//! Purpose:
//! Declarative eval registry entry for `stream_socket_recvfrom`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference address path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_recvfrom",
    area: Filesystem,
    params: [
        socket,
        length,
        flags = EvalBuiltinDefaultValue::Int(0),
        address: by_ref = EvalBuiltinDefaultValue::String("")
    ],
    by_ref: [address],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_socket_recvfrom` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_socket_recvfrom", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_socket_recvfrom` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_socket_recvfrom", evaluated_args, context, values)
}
