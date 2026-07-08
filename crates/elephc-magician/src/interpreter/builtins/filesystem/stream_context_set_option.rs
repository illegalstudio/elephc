//! Purpose:
//! Declarative eval registry entry for `stream_context_set_option`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Owns both `stream_context_set_option($context, $options)` and the
//!   four-argument nested option form.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_set_option",
    area: Filesystem,
    params: [
        context,
        wrapper_or_options,
        option_name = EvalBuiltinDefaultValue::Null,
        value = EvalBuiltinDefaultValue::Null
    ],
    required: 2,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_context_set_option($context, ...)`.
pub(in crate::interpreter) fn eval_stream_context_set_option_declared_call(
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

/// Stores context options from already evaluated arguments.
pub(in crate::interpreter) fn eval_stream_context_set_option_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream_context, options] => {
            eval_stream_context_set_options_result(*stream_context, *options, context, values)
        }
        [stream_context, wrapper, option, value] => eval_stream_context_set_option_result(
            *stream_context,
            *wrapper,
            *option,
            *value,
            context,
            values,
        ),
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
    let options = super::stream_context_create::eval_stream_context_options_arg(Some(options), values)?;
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

/// Converts a runtime resource cell into eval's zero-based stream context id.
pub(in crate::interpreter) fn eval_stream_context_resource_id(
    stream_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(stream_context)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(stream_context, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
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
