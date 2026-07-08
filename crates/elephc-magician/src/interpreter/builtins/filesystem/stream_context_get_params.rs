//! Purpose:
//! Declarative eval registry entry for `stream_context_get_params`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Eval validates the context resource and mirrors the main backend's empty-params behavior.

eval_builtin! {
    name: "stream_context_get_params",
    area: Filesystem,
    params: [context],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_context_get_params($context)` to an empty associative array.
pub(in crate::interpreter) fn eval_stream_context_get_params_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream_context = eval_expr(stream_context, context, scope, values)?;
    eval_stream_context_get_params_result(stream_context, values)
}

/// Returns empty params for an already evaluated stream context resource.
pub(in crate::interpreter) fn eval_stream_context_get_params_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_context_get_params_result(*stream_context, values)
}

/// Validates the stream context resource and returns the current empty params array.
pub(in crate::interpreter) fn eval_stream_context_get_params_result(
    stream_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_context_set_option::eval_stream_context_resource_id(stream_context, values)?;
    values.assoc_new(0)
}
