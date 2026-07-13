//! Purpose:
//! Declarative eval registry entry for `stream_get_filters`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the static stream-filter list helper.

eval_builtin! {
    name: "stream_get_filters",
    area: String,
    params: [],
    direct: StreamIntrospection,
    values: StreamIntrospection,
}

use super::super::super::*;

/// Evaluates PHP `stream_get_filters()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_stream_get_filters(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_wrappers::eval_builtin_stream_introspection_named("stream_get_filters", args, context, values)
}

/// Builds the result for PHP `stream_get_filters()`.
pub(in crate::interpreter) fn eval_stream_get_filters_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_wrappers::eval_stream_introspection_named_result("stream_get_filters", context, values)
}
