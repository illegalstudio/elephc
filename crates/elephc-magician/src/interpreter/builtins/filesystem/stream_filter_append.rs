//! Purpose:
//! Declarative eval registry entry and implementation for `stream_filter_append`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Creates eval-local filter resources without transforming stream bytes.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_filter_append",
    area: Filesystem,
    params: [
        stream,
        filtername,
        read_write = EvalBuiltinDefaultValue::Int(3),
        params = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_filter_append($stream, $filtername, $read_write = 3, $params = null)`.
pub(in crate::interpreter) fn eval_stream_filter_append_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let filter_name = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_stream_filter_attach_result("stream_filter_append", stream, filter_name, context, values)
}

/// Appends a filter from already evaluated stream filter arguments.
pub(in crate::interpreter) fn eval_stream_filter_append_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_filter_attach_result(
        "stream_filter_append",
        evaluated_args[0],
        evaluated_args[1],
        context,
        values,
    )
}

/// Creates an eval-local filter resource for a materialized stream filter attach.
pub(in crate::interpreter) fn eval_stream_filter_attach_result(
    name: &str,
    stream: RuntimeCellHandle,
    filter_name: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(name, "stream_filter_append" | "stream_filter_prepend") {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream_id = super::stream_bucket_new::eval_stream_extension_resource_id(stream, values)?;
    let _ = values.string_bytes(filter_name)?;
    if !context.stream_resources().has_stream(stream_id) {
        return values.bool_value(false);
    }
    let filter_id = context.stream_resources_mut().open_filter_resource();
    values.resource(filter_id)
}
