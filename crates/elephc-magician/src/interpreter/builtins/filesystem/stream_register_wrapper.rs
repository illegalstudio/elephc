//! Purpose:
//! Declarative eval registry entry for `stream_register_wrapper`, PHP's alias of `stream_wrapper_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The alias needs its OWN inventory entry: the registry panics on a duplicate name,
//!   so it cannot be a second declaration of `stream_wrapper_register`.
//! - Parameter names follow php-src (protocol, class, flags), not the canonical builtin's, because the
//!   parity gate compares them against the compiler's static signature.
//! - Both dispatch arms delegate straight to the canonical implementation.

eval_builtin! {
    contract: "stream_register_wrapper",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `stream_register_wrapper` through the canonical `stream_wrapper_register` implementation.
pub(in crate::interpreter) fn eval_stream_register_wrapper_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_wrapper_register::eval_stream_wrapper_register_declared_call(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for `stream_register_wrapper` through the canonical `stream_wrapper_register` implementation.
pub(in crate::interpreter) fn eval_stream_register_wrapper_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_wrapper_register::eval_stream_wrapper_register_declared_values_result(evaluated_args, context, values)
}
