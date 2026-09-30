//! Purpose:
//! Declarative eval registry entry for `set_file_buffer`, PHP's alias of `stream_set_write_buffer`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The alias needs its OWN inventory entry: the registry panics on a duplicate name,
//!   so it cannot be a second declaration of `stream_set_write_buffer`.
//! - Parameter names follow php-src (stream, size), not the canonical builtin's, because the
//!   parity gate compares them against the compiler's static signature.
//! - Both dispatch arms delegate straight to the canonical implementation.

eval_builtin! {
    contract: "set_file_buffer",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `set_file_buffer` through the canonical `stream_set_write_buffer` implementation.
pub(in crate::interpreter) fn eval_set_file_buffer_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_set_write_buffer::eval_stream_set_write_buffer_declared_call(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for `set_file_buffer` through the canonical `stream_set_write_buffer` implementation.
pub(in crate::interpreter) fn eval_set_file_buffer_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_set_write_buffer::eval_stream_set_write_buffer_declared_values_result(evaluated_args, context, values)
}
