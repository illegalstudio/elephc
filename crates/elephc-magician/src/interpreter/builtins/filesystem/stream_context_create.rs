//! Purpose:
//! Declarative eval registry entry for `stream_context_create`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Owns context resource creation and validates optional options arrays before storage.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_create",
    area: Filesystem,
    params: [
        options = EvalBuiltinDefaultValue::Null,
        params = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_context_create($options = null, $params = null)`.
pub(in crate::interpreter) fn eval_stream_context_create_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let options = match args.first() {
        Some(options) => Some(eval_expr(options, context, scope, values)?),
        None => None,
    };
    if let Some(params) = args.get(1) {
        eval_expr(params, context, scope, values)?;
    }
    eval_stream_context_create_result(options, context, values)
}

/// Creates a stream context resource from already evaluated optional options.
pub(in crate::interpreter) fn eval_stream_context_create_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_stream_context_create_result(None, context, values),
        [options] => eval_stream_context_create_result(Some(*options), context, values),
        [options, _params] => eval_stream_context_create_result(Some(*options), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates a stream context resource from materialized optional options.
pub(in crate::interpreter) fn eval_stream_context_create_result(
    options: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let options = eval_stream_context_options_arg(options, values)?;
    let id = context.stream_resources_mut().open_stream_context(options);
    values.resource(id)
}

/// Converts an optional options argument into a stored context option handle.
pub(in crate::interpreter) fn eval_stream_context_options_arg(
    options: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(options) = options else {
        return Ok(None);
    };
    if values.type_tag(options)? == EVAL_TAG_NULL {
        return Ok(None);
    }
    if !values.is_array_like(options)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(Some(options))
}
