//! Purpose:
//! Declarative eval registry entry for `stream_get_wrappers`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the eval stream-wrapper registry helper.

eval_builtin! {
    name: "stream_get_wrappers",
    area: String,
    params: [],
    direct: StreamIntrospection,
    values: StreamIntrospection,
}

use super::super::super::*;

/// Evaluates PHP `stream_get_wrappers()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_stream_get_wrappers(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_wrappers::eval_builtin_stream_introspection_named("stream_get_wrappers", args, context, values)
}

/// Builds the result for PHP `stream_get_wrappers()`.
pub(in crate::interpreter) fn eval_stream_get_wrappers_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_get_wrappers::eval_stream_introspection_named_result("stream_get_wrappers", context, values)
}

/// Evaluates a named stream introspection builtin with no arguments.
pub(in crate::interpreter) fn eval_builtin_stream_introspection_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_introspection_named_result(name, context, values)
}

/// Builds the static list returned by one eval stream introspection builtin.
pub(in crate::interpreter) fn eval_stream_introspection_named_result(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let items = match name {
        "stream_get_filters" => return eval_static_string_array_result(EVAL_STREAM_FILTERS, values),
        "stream_get_transports" => {
            return eval_static_string_array_result(EVAL_STREAM_TRANSPORTS, values);
        }
        "stream_get_wrappers" => context.stream_resources().stream_wrappers(EVAL_STREAM_WRAPPERS),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_owned_string_array_result(&items, values)
}

/// Builds one indexed PHP array from an owned string slice.
pub(in crate::interpreter) fn eval_owned_string_array_result(
    items: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(items.len())?;
    for (index, item) in items.iter().enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string(item)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}
