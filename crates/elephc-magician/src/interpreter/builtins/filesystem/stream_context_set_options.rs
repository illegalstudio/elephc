//! Purpose:
//! Declarative eval registry entry for `stream_context_set_options`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Delegates to the shared two-argument stream-context option store.

eval_builtin! {
    contract: "stream_context_set_options",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::stream_context_set_option::eval_stream_context_set_options_result;

/// Evaluates `stream_context_set_options($context, $options)`.
pub(in crate::interpreter) fn eval_stream_context_set_options_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context, options] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream_context = eval_expr(stream_context, context, scope, values)?;
    let options = eval_expr(options, context, scope, values)?;
    eval_stream_context_set_options_result(stream_context, options, context, values)
}

/// Stores options from already evaluated arguments on a stream context resource.
pub(in crate::interpreter) fn eval_stream_context_set_options_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context, options] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_context_set_options_result(*stream_context, *options, context, values)
}
