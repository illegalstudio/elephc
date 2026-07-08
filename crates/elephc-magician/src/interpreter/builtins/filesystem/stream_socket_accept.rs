//! Purpose:
//! Declarative eval registry entry for `stream_socket_accept`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference peer-name path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_accept",
    area: Filesystem,
    params: [
        socket,
        timeout = EvalBuiltinDefaultValue::Null,
        peer_name: by_ref = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [peer_name],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_socket_accept` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_socket_accept_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_socket_accept", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_socket_accept` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_socket_accept_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_socket_accept", evaluated_args, context, values)
}
