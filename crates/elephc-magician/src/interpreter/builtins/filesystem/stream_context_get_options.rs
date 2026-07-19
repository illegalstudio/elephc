//! Purpose:
//! Declarative eval registry entry for `stream_context_get_options`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Returns persisted context options or an empty associative array.

eval_builtin! {
    name: "stream_context_get_options",
    area: Filesystem,
    params: [context],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_context_get_options($context)`.
pub(in crate::interpreter) fn eval_stream_context_get_options_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream_context = eval_expr(stream_context, context, scope, values)?;
    eval_stream_context_get_options_result(stream_context, context, values)
}

/// Returns options for an already evaluated stream context resource.
pub(in crate::interpreter) fn eval_stream_context_get_options_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_context_get_options_result(*stream_context, context, values)
}

/// Returns persisted stream context options or an empty associative array.
pub(in crate::interpreter) fn eval_stream_context_get_options_result(
    stream_context: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::stream_context_set_option::eval_stream_context_resource_id(stream_context, values)?;
    match context.stream_resources().stream_context_options(id) {
        Some(options) => Ok(options),
        None => values.assoc_new(0),
    }
}
