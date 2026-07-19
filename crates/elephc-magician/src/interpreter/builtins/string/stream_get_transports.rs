//! Purpose:
//! Declarative eval registry entry for `stream_get_transports`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the static stream-transport list helper.

eval_builtin! {
    name: "stream_get_transports",
    area: String,
    params: [],
    direct: StreamIntrospection,
    values: StreamIntrospection,
}

use super::super::super::*;

/// Evaluates PHP `stream_get_transports()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_stream_get_transports(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_wrappers::eval_builtin_stream_introspection_named("stream_get_transports", args, context, values)
}

/// Builds the result for PHP `stream_get_transports()`.
pub(in crate::interpreter) fn eval_stream_get_transports_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_wrappers::eval_stream_introspection_named_result("stream_get_transports", context, values)
}
