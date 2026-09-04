//! Purpose:
//! Declarative eval registry entry for `stream_context_set_options`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The two-argument array spelling PHP 8.3 added. The singular name still accepts the same
//!   shape, so both reach `eval_stream_context_set_options_result`; this entry exists because
//!   the parity gate requires every static builtin to be visible to `eval()` by name.

eval_builtin! {
    contract: "stream_context_set_options",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

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
    super::stream_context_set_option::eval_stream_context_set_options_result(
        stream_context,
        options,
        context,
        values,
    )
}

/// Dispatches evaluated-argument calls for `stream_context_set_options`.
pub(in crate::interpreter) fn eval_stream_context_set_options_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream_context, options] => {
            super::stream_context_set_option::eval_stream_context_set_options_result(
                *stream_context,
                *options,
                context,
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
