//! Purpose:
//! Implements eval stream context metadata builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Eval stores options per context resource, while `get_params()` mirrors the
//!   main backend's current empty-array behavior.
//! - Context resources share the same generic resource id namespace as streams.

use super::super::super::*;

/// Evaluates `stream_context_create($options = null, $params = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_context_create(
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

/// Evaluates `stream_context_get_default($options = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_context_get_default(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        eval_expr(arg, context, scope, values)?;
    }
    eval_stream_context_get_default_result(context, values)
}

/// Returns eval's default stream context resource.
pub(in crate::interpreter) fn eval_stream_context_get_default_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = context.stream_resources_mut().default_stream_context();
    values.resource(id)
}

/// Evaluates `stream_context_set_default($options)`.
pub(in crate::interpreter) fn eval_builtin_stream_context_set_default(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [options] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_expr(options, context, scope, values)?;
    eval_stream_context_get_default_result(context, values)
}

/// Evaluates `stream_context_set_option($context, ...)`.
pub(in crate::interpreter) fn eval_builtin_stream_context_set_option(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [stream_context, options] => {
            let stream_context = eval_expr(stream_context, context, scope, values)?;
            let options = eval_expr(options, context, scope, values)?;
            eval_stream_context_set_options_result(stream_context, options, context, values)
        }
        [stream_context, wrapper, option, value] => {
            let stream_context = eval_expr(stream_context, context, scope, values)?;
            let wrapper = eval_expr(wrapper, context, scope, values)?;
            let option = eval_expr(option, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            eval_stream_context_set_option_result(
                stream_context,
                wrapper,
                option,
                value,
                context,
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Stores a materialized options array on a stream context resource.
pub(in crate::interpreter) fn eval_stream_context_set_options_result(
    stream_context: RuntimeCellHandle,
    options: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_context_resource_id(stream_context, values)?;
    let options = eval_stream_context_options_arg(Some(options), values)?;
    values.bool_value(
        context
            .stream_resources_mut()
            .set_stream_context_options(id, options),
    )
}

/// Stores one nested `options[wrapper][option] = value` entry on a stream context.
pub(in crate::interpreter) fn eval_stream_context_set_option_result(
    stream_context: RuntimeCellHandle,
    wrapper: RuntimeCellHandle,
    option: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_context_resource_id(stream_context, values)?;
    let wrapper = values.cast_string(wrapper)?;
    let option = values.cast_string(option)?;
    let options = match context.stream_resources().stream_context_options(id) {
        Some(options) => options,
        None => values.assoc_new(1)?,
    };
    let wrapper_options = eval_stream_context_wrapper_options(options, wrapper, values)?;
    let wrapper_options = values.array_set(wrapper_options, option, value)?;
    let options = values.array_set(options, wrapper, wrapper_options)?;
    values.bool_value(
        context
            .stream_resources_mut()
            .set_stream_context_options(id, Some(options)),
    )
}

/// Evaluates `stream_context_set_params($context, $params)` as an accepted no-op.
pub(in crate::interpreter) fn eval_builtin_stream_context_set_params(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context, params] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_expr(stream_context, context, scope, values)?;
    eval_expr(params, context, scope, values)?;
    values.bool_value(true)
}

/// Evaluates `stream_context_get_options($context)`.
pub(in crate::interpreter) fn eval_builtin_stream_context_get_options(
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

/// Returns persisted stream context options or an empty associative array.
pub(in crate::interpreter) fn eval_stream_context_get_options_result(
    stream_context: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_context_resource_id(stream_context, values)?;
    match context.stream_resources().stream_context_options(id) {
        Some(options) => Ok(options),
        None => values.assoc_new(0),
    }
}

/// Evaluates `stream_context_get_params($context)` to an empty associative array.
pub(in crate::interpreter) fn eval_builtin_stream_context_get_params(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_context] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_expr(stream_context, context, scope, values)?;
    values.assoc_new(0)
}

/// Converts an optional options argument into a stored context option handle.
fn eval_stream_context_options_arg(
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

/// Returns the nested wrapper options array, creating one when missing or scalar.
fn eval_stream_context_wrapper_options(
    options: RuntimeCellHandle,
    wrapper: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = values.array_key_exists(wrapper, options)?;
    if values.truthy(exists)? {
        let wrapper_options = values.array_get(options, wrapper)?;
        if values.is_array_like(wrapper_options)? {
            return Ok(wrapper_options);
        }
    }
    values.assoc_new(1)
}

/// Converts a runtime resource cell into eval's zero-based stream context id.
fn eval_stream_context_resource_id(
    stream_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(stream_context)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(stream_context, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
