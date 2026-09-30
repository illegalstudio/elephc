//! Purpose:
//! Declarative eval registry entry for `socket_set_timeout`, PHP's alias of `stream_set_timeout`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The alias needs its OWN inventory entry: the registry panics on a duplicate name,
//!   so it cannot be a second declaration of `stream_set_timeout`.
//! - Parameter names follow php-src (stream, seconds, microseconds), not the canonical builtin's, because the
//!   parity gate compares them against the compiler's static signature.
//! - Both dispatch arms delegate straight to the canonical implementation.

eval_builtin! {
    contract: "socket_set_timeout",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `socket_set_timeout` through the canonical `stream_set_timeout` implementation.
pub(in crate::interpreter) fn eval_socket_set_timeout_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_set_timeout::eval_stream_set_timeout_declared_call(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for `socket_set_timeout` through the canonical `stream_set_timeout` implementation.
pub(in crate::interpreter) fn eval_socket_set_timeout_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_set_timeout::eval_stream_set_timeout_declared_values_result(evaluated_args, context, values)
}
