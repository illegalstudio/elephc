//! Purpose:
//! Declarative eval registry entry for `socket_set_blocking`, PHP's alias of `stream_set_blocking`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The alias needs its OWN inventory entry: the registry panics on a duplicate name,
//!   so it cannot be a second declaration of `stream_set_blocking`.
//! - Parameter names follow php-src (stream, enable), not the canonical builtin's, because the
//!   parity gate compares them against the compiler's static signature.
//! - Both dispatch arms delegate straight to the canonical implementation.

eval_builtin! {
    contract: "socket_set_blocking",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `socket_set_blocking` through the canonical `stream_set_blocking` implementation.
pub(in crate::interpreter) fn eval_socket_set_blocking_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_set_blocking::eval_stream_set_blocking_declared_call(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for `socket_set_blocking` through the canonical `stream_set_blocking` implementation.
pub(in crate::interpreter) fn eval_socket_set_blocking_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_set_blocking::eval_stream_set_blocking_declared_values_result(evaluated_args, context, values)
}
