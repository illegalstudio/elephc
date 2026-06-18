//! Purpose:
//! Implements eval stream wrapper, stream filter, and stream bucket helper builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Wrapper/filter registries are conservative eval stubs matching the main
//!   backend surface without changing stream bytes.
//! - Buckets are represented as `stdClass` objects with `data`, `datalen`, and
//!   brigade `_buckets` properties, mirroring the generated runtime model.

use super::super::super::*;

/// Evaluates stream wrapper registration builtins.
pub(in crate::interpreter) fn eval_builtin_stream_wrapper_registry(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "stream_wrapper_register" if (2..=3).contains(&args.len()) => {}
        "stream_wrapper_unregister" | "stream_wrapper_restore" if args.len() == 1 => {}
        _ => return Err(EvalStatus::RuntimeFatal),
    }
    for arg in args {
        let value = eval_expr(arg, context, scope, values)?;
        if matches!(name, "stream_wrapper_register" | "stream_wrapper_unregister" | "stream_wrapper_restore") {
            let _ = values.string_bytes(value).ok();
        }
    }
    values.bool_value(true)
}

/// Evaluates stream wrapper registration builtins from materialized arguments.
pub(in crate::interpreter) fn eval_stream_wrapper_registry_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "stream_wrapper_register" if (2..=3).contains(&evaluated_args.len()) => {}
        "stream_wrapper_unregister" | "stream_wrapper_restore" if evaluated_args.len() == 1 => {}
        _ => return Err(EvalStatus::RuntimeFatal),
    }
    for value in evaluated_args {
        let _ = values.string_bytes(*value).ok();
    }
    values.bool_value(true)
}

/// Evaluates `stream_filter_register(filter_name, class)`.
pub(in crate::interpreter) fn eval_builtin_stream_filter_register(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filter_name, class] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filter_name = eval_expr(filter_name, context, scope, values)?;
    let class = eval_expr(class, context, scope, values)?;
    eval_stream_filter_register_result(filter_name, class, values)
}

/// Evaluates a materialized `stream_filter_register()` call.
pub(in crate::interpreter) fn eval_stream_filter_register_result(
    filter_name: RuntimeCellHandle,
    class: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let _ = values.string_bytes(filter_name)?;
    let _ = values.string_bytes(class)?;
    values.bool_value(true)
}

/// Evaluates `stream_filter_append()` or `stream_filter_prepend()`.
pub(in crate::interpreter) fn eval_builtin_stream_filter_attach(
    name: &str,
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
    eval_stream_filter_attach_result(name, stream, filter_name, context, values)
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
    let stream_id = eval_stream_extension_resource_id(stream, values)?;
    let _ = values.string_bytes(filter_name)?;
    if !context.stream_resources().has_stream(stream_id) {
        return values.bool_value(false);
    }
    let filter_id = context.stream_resources_mut().open_filter_resource();
    values.resource(filter_id)
}

/// Evaluates `stream_filter_remove(stream_filter)`.
pub(in crate::interpreter) fn eval_builtin_stream_filter_remove(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_filter] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream_filter = eval_expr(stream_filter, context, scope, values)?;
    eval_stream_filter_remove_result(stream_filter, context, values)
}

/// Removes an eval-local filter resource.
pub(in crate::interpreter) fn eval_stream_filter_remove_result(
    stream_filter: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_extension_resource_id(stream_filter, values)?;
    values.bool_value(context.stream_resources_mut().close_filter_resource(id))
}

/// Evaluates `stream_bucket_new(stream, buffer)`.
pub(in crate::interpreter) fn eval_builtin_stream_bucket_new(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, buffer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let buffer = eval_expr(buffer, context, scope, values)?;
    eval_stream_bucket_new_result(stream, buffer, context, values)
}

/// Creates a stdClass-backed stream bucket object.
pub(in crate::interpreter) fn eval_stream_bucket_new_result(
    stream: RuntimeCellHandle,
    buffer: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let stream_id = eval_stream_extension_resource_id(stream, values)?;
    if !context.stream_resources().has_stream(stream_id) {
        return values.null();
    }
    let bytes = values.string_bytes(buffer)?;
    let bucket = values.new_object("stdClass")?;
    let data = values.string_bytes_value(&bytes)?;
    let datalen = values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    values.property_set(bucket, "data", data)?;
    values.property_set(bucket, "datalen", datalen)?;
    Ok(bucket)
}

/// Evaluates `stream_bucket_make_writeable(brigade)`.
pub(in crate::interpreter) fn eval_builtin_stream_bucket_make_writeable(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [brigade] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let brigade = eval_expr(brigade, context, scope, values)?;
    eval_stream_bucket_make_writeable_result(brigade, values)
}

/// Returns the first bucket in a brigade, or null when none exists.
pub(in crate::interpreter) fn eval_stream_bucket_make_writeable_result(
    brigade: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let buckets = values.property_get(brigade, "_buckets")?;
    if !values.is_array_like(buckets)? || values.array_len(buckets)? == 0 {
        return values.null();
    }
    let key = values.int(0)?;
    values.array_get(buckets, key)
}

/// Evaluates `stream_bucket_append()` or `stream_bucket_prepend()`.
pub(in crate::interpreter) fn eval_builtin_stream_bucket_push(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [brigade, bucket] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let brigade = eval_expr(brigade, context, scope, values)?;
    let bucket = eval_expr(bucket, context, scope, values)?;
    eval_stream_bucket_push_result(name, brigade, bucket, values)
}

/// Adds a bucket object to the brigade's `_buckets` array.
pub(in crate::interpreter) fn eval_stream_bucket_push_result(
    name: &str,
    brigade: RuntimeCellHandle,
    bucket: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let prepend = match name {
        "stream_bucket_append" => false,
        "stream_bucket_prepend" => true,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let buckets = eval_brigade_buckets(brigade, values)?;
    let buckets = if prepend {
        eval_bucket_prepend(buckets, bucket, values)?
    } else {
        let len = values.array_len(buckets)?;
        let index = values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        values.array_set(buckets, index, bucket)?
    };
    values.property_set(brigade, "_buckets", buckets)?;
    values.null()
}

/// Returns an existing brigade bucket array or creates an empty one.
fn eval_brigade_buckets(
    brigade: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let buckets = values.property_get(brigade, "_buckets")?;
    if values.is_array_like(buckets)? {
        Ok(buckets)
    } else {
        values.array_new(0)
    }
}

/// Builds a new bucket array with the provided bucket at index zero.
fn eval_bucket_prepend(
    buckets: RuntimeCellHandle,
    bucket: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(buckets)?;
    let mut result = values.array_new(len + 1)?;
    let zero = values.int(0)?;
    result = values.array_set(result, zero, bucket)?;
    for index in 0..len {
        let old_key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = values.array_get(buckets, old_key)?;
        let new_key =
            values.int(i64::try_from(index + 1).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, new_key, value)?;
    }
    Ok(result)
}

/// Converts a runtime resource cell into eval's zero-based stream-extension id.
fn eval_stream_extension_resource_id(
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(resource, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
