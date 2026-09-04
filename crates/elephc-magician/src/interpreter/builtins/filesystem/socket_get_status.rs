//! Purpose:
//! Declarative eval registry entry for `socket_get_status`, PHP's alias of `stream_get_meta_data`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The alias needs its OWN inventory entry: the registry panics on a duplicate name,
//!   so it cannot be a second declaration of `stream_get_meta_data`.
//! - Parameter names follow php-src (stream), not the canonical builtin's, because the
//!   parity gate compares them against the compiler's static signature.
//! - Both dispatch arms delegate straight to the canonical implementation.

eval_builtin! {
    contract: "socket_get_status",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `socket_get_status` through the canonical `stream_get_meta_data` implementation.
pub(in crate::interpreter) fn eval_socket_get_status_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_meta_data::eval_stream_get_meta_data_declared_call(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for `socket_get_status` through the canonical `stream_get_meta_data` implementation.
pub(in crate::interpreter) fn eval_socket_get_status_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_meta_data::eval_stream_get_meta_data_declared_values_result(evaluated_args, context, values)
}
