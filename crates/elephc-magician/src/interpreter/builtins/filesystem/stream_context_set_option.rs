//! Purpose:
//! Declarative eval registry entry for `stream_context_set_option`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Owns both `stream_context_set_option($context, $options)` and the
//!   four-argument nested option form.

eval_builtin! {
    contract: "stream_context_set_option",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use crate::interpreter::builtins::collection_builder::with_collection_call;
use elephc_memoir::runtime_owners::StagedRuntimeOwners;
use super::super::super::*;

/// Evaluates `stream_context_set_option($context, ...)`.
pub(in crate::interpreter) fn eval_stream_context_set_option_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [stream_context, options] => with_eval_operands(
            &[stream_context, options],
            context,
            scope,
            values,
            |operands, context, _, values| {
                eval_stream_context_set_options_result(
                    operands[0].borrowed(),
                    operands[1].borrowed(),
                    context,
                    values,
                )
            },
        ),
        [stream_context, wrapper, option, value] => with_eval_operands(
            &[stream_context, wrapper, option, value],
            context,
            scope,
            values,
            |operands, context, _, values| {
                eval_stream_context_set_option_result(
                    operands[0].borrowed(),
                    operands[1].borrowed(),
                    operands[2].borrowed(),
                    operands[3].borrowed(),
                    context,
                    values,
                )
            },
        ),
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
    let stored = eval_replace_stream_context_options(id, options, context, values)?;
    values.bool_value(stored)
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
    with_collection_call(context, values, |context, values, owners, result| {
        let wrapper = owners.stage(values.cast_string(wrapper)?);
        let option = owners.stage(values.cast_string(option)?);
        let options = match context.stream_resources().stream_context_options(id) {
            Some(options) => owners.stage(values.retain(options)?),
            None => owners.stage(values.assoc_new(1)?),
        };
        let wrapper_options = eval_stream_context_wrapper_options(options, wrapper, values, owners)?;
        let wrapper_options = values.array_set(wrapper_options, option, value)?;
        let options = values.array_set(options, wrapper, wrapper_options)?;
        let options = values.retain(options.borrowed())?;
        let stored = eval_replace_stream_context_options(id, Some(options), context, values)?;
        result.replace(Some(values.bool_value(stored)?));
        Ok(())
    })
}

/// Publishes an independently owned option cell before releasing the detached predecessor.
fn eval_replace_stream_context_options(
    id: i64,
    options: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let (stored, detached) = match context
        .stream_resources_mut()
        .replace_stream_context_options(id, options)
    {
        Ok(previous) => (true, previous),
        Err(incoming) => (false, incoming),
    };
    if let Some(detached) = detached {
        eval_release_value(context, values, detached)?;
    }
    Ok(stored)
}

/// Converts a runtime resource cell into eval's zero-based stream context id.
pub(in crate::interpreter) fn eval_stream_context_resource_id(
    stream_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    eval_resource_payload(stream_context, values)
}

/// Returns the nested wrapper options array, creating one when missing or scalar.
fn eval_stream_context_wrapper_options(
    options: RuntimeCellHandle,
    wrapper: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    owners: &StagedRuntimeOwners,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = owners.stage(values.array_key_exists(wrapper, options)?);
    if values.truthy(exists)? {
        let wrapper_options = owners.stage(values.array_get(options, wrapper)?);
        if values.is_array_like(wrapper_options)? {
            return Ok(wrapper_options);
        }
    }
    Ok(owners.stage(values.assoc_new(1)?))
}
