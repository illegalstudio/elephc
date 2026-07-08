//! Purpose:
//! Declarative eval registry entry for `stream_context_set_params`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Eval validates the context resource and accepts params as a no-op.

eval_builtin! {
    name: "stream_context_set_params",
    area: Filesystem,
    params: [context, params],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_context_set_params($context, $params)` as an accepted no-op.
pub(in crate::interpreter) fn eval_stream_context_set_params_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context, params] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream_context = eval_expr(stream_context, context, scope, values)?;
    eval_expr(params, context, scope, values)?;
    eval_stream_context_set_params_result(stream_context, values)
}

/// Returns true after validating already evaluated stream context params.
pub(in crate::interpreter) fn eval_stream_context_set_params_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context, _params] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_context_set_params_result(*stream_context, values)
}

/// Validates the stream context resource and returns true for accepted params.
pub(in crate::interpreter) fn eval_stream_context_set_params_result(
    stream_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_context_set_option::eval_stream_context_resource_id(stream_context, values)?;
    values.bool_value(true)
}
