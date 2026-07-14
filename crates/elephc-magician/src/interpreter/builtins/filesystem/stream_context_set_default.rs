//! Purpose:
//! Declarative eval registry entry for `stream_context_set_default`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Current eval behavior validates arity/evaluation and returns the default context resource.

eval_builtin! {
    name: "stream_context_set_default",
    area: Filesystem,
    params: [options],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_context_set_default($options)`.
pub(in crate::interpreter) fn eval_stream_context_set_default_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [options] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_expr(options, context, scope, values)?;
    super::stream_context_get_default::eval_stream_context_get_default_result(context, values)
}

/// Returns the default context after validating already evaluated arguments.
pub(in crate::interpreter) fn eval_stream_context_set_default_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [_options] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    super::stream_context_get_default::eval_stream_context_get_default_result(context, values)
}
