//! Purpose:
//! Declarative eval registry entry for `stream_supports_lock`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the stream boolean predicate helper.

eval_builtin! {
    name: "stream_supports_lock",
    area: String,
    params: [stream],
    direct: StreamBoolPredicate,
    values: StreamBoolPredicate,
}

use super::super::super::*;

/// Evaluates PHP `stream_supports_lock(...)` over one stream expression.
pub(in crate::interpreter) fn eval_builtin_stream_supports_lock(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_is_local::eval_builtin_stream_bool_predicate_named("stream_supports_lock", args, context, scope, values)
}

/// Builds the result for PHP `stream_supports_lock(...)` from one evaluated stream value.
pub(in crate::interpreter) fn eval_stream_supports_lock_result(
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_is_local::eval_stream_bool_predicate_named_result("stream_supports_lock", stream, values)
}
