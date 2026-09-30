//! Purpose:
//! Declarative eval registry entry for `stream_context_get_options`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Returns persisted context options or an empty associative array.

eval_builtin! {
    contract: "stream_context_get_options",
    area: Filesystem,
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
    // php-src raises a catchable TypeError that names its own stub parameter —
    // `$stream_or_context`, not `$stream` — and the VALUE's own type spelling. Reaching
    // `eval_resource_payload()` with a non-resource used to die as an uncatchable runtime fatal.
    if values.type_tag(stream_context)? != EVAL_TAG_RESOURCE {
        let given = eval_stream_php_type_name(stream_context, values)?;
        let message = format!(
            "stream_context_get_options(): Argument #1 ($stream_or_context) must be of type \
             resource, {} given",
            given
        );
        return eval_stream_type_error(&message, context, values);
    }
    let id = super::stream_context_set_option::eval_stream_context_resource_id(stream_context, values)?;
    match context.stream_resources().stream_context_options(id) {
        Some(options) => Ok(options),
        None => values.assoc_new(0),
    }
}
