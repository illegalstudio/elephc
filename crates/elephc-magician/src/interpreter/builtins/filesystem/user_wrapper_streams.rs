//! Purpose:
//! Dispatches eval userspace stream wrapper resources into wrapper class methods.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::streams` for `fopen()`,
//!   `fread()`, `fwrite()`, `feof()`, and `fclose()`.
//!
//! Key details:
//! - Registered wrapper resources keep the wrapper object in eval-local resource
//!   state; file-backed streams continue to use the normal host-file path.
//! - `stream_open()` receives a synthetic by-ref `opened_path` cell. Magician
//!   accepts writes to it, but currently keeps the original URL as the stream URI.

use super::super::super::*;

/// Opens a registered eval userspace stream wrapper, or reports no match.
pub(in crate::interpreter) fn eval_user_wrapper_fopen_result(
    filename: &str,
    mode: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class_name) = context
        .stream_resources()
        .user_stream_wrapper_class_for_path(filename)
    else {
        return Ok(None);
    };
    let Some(class) = context.class(&class_name).cloned() else {
        return values.bool_value(false).map(Some);
    };
    let Some((declaring_class, stream_open)) =
        eval_user_wrapper_method(class.name(), "stream_open", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let object = eval_dynamic_class_new_object(&class, Vec::new(), context, scope, values)?;
    let open_args = eval_user_wrapper_stream_open_args(filename, mode, values)?;
    let open_result = eval_dynamic_method_with_values(
        &declaring_class,
        class.name(),
        &stream_open,
        object,
        open_args,
        context,
        values,
    )?;
    let opened = values.truthy(open_result)?;
    values.release(open_result)?;
    if !opened {
        values.release(object)?;
        return values.bool_value(false).map(Some);
    }
    let id =
        context
            .stream_resources_mut()
            .open_user_wrapper_stream(object, class.name(), filename, mode);
    values.resource(id).map(Some)
}

/// Dispatches `fclose()` to `stream_close()` for a userspace-wrapper stream.
pub(in crate::interpreter) fn eval_user_wrapper_fclose_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    if let Some((declaring_class, stream_close)) =
        eval_user_wrapper_method(&info.class_name, "stream_close", context)
    {
        let result = eval_dynamic_method_with_values(
            &declaring_class,
            &info.class_name,
            &stream_close,
            info.object,
            Vec::new(),
            context,
            values,
        )?;
        values.release(result)?;
    }
    let closed = context.stream_resources_mut().close(id);
    values.release(info.object)?;
    values.bool_value(closed).map(Some)
}

/// Dispatches `fread()` or `fgetc()` to a wrapper object's `stream_read()`.
pub(in crate::interpreter) fn eval_user_wrapper_fread_result(
    id: i64,
    length: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_read)) =
        eval_user_wrapper_method(&info.class_name, "stream_read", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let length = values.int(i64::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_read,
        info.object,
        positional_args(vec![length]),
        context,
        values,
    )?;
    let bytes = values.string_bytes(result)?;
    values.release(result)?;
    context
        .stream_resources_mut()
        .set_user_wrapper_eof(id, bytes.is_empty());
    values.string_bytes_value(&bytes).map(Some)
}

/// Dispatches `fwrite()` to a wrapper object's `stream_write()`.
pub(in crate::interpreter) fn eval_user_wrapper_fwrite_result(
    id: i64,
    data: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_write)) =
        eval_user_wrapper_method(&info.class_name, "stream_write", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let data = values.string_bytes_value(data)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_write,
        info.object,
        positional_args(vec![data]),
        context,
        values,
    )?;
    let written = eval_int_value(result, values)?;
    values.release(result)?;
    context.stream_resources_mut().set_user_wrapper_eof(id, false);
    values.int(written).map(Some)
}

/// Dispatches `feof()` to a wrapper object's `stream_eof()`.
pub(in crate::interpreter) fn eval_user_wrapper_feof_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_eof)) =
        eval_user_wrapper_method(&info.class_name, "stream_eof", context)
    else {
        return values.bool_value(info.eof).map(Some);
    };
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_eof,
        info.object,
        Vec::new(),
        context,
        values,
    )?;
    let eof = values.truthy(result)?;
    values.release(result)?;
    context.stream_resources_mut().set_user_wrapper_eof(id, eof);
    values.bool_value(eof).map(Some)
}

/// Builds the four PHP arguments passed to a wrapper `stream_open()` method.
fn eval_user_wrapper_stream_open_args(
    filename: &str,
    mode: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let path = values.string(filename)?;
    let mode = values.string(mode)?;
    let options = values.int(0)?;
    let opened_path = values.null()?;
    Ok(vec![
        EvaluatedCallArg {
            name: None,
            value: path,
            ref_target: None,
        },
        EvaluatedCallArg {
            name: None,
            value: mode,
            ref_target: None,
        },
        EvaluatedCallArg {
            name: None,
            value: options,
            ref_target: None,
        },
        EvaluatedCallArg {
            name: None,
            value: opened_path,
            ref_target: Some(EvalReferenceTarget::Cell { cell: opened_path }),
        },
    ])
}

/// Returns a callable eval-declared wrapper method.
fn eval_user_wrapper_method(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    let (declaring_class, method) = context.class_method(class_name, method_name)?;
    if method.visibility() != EvalVisibility::Public || method.is_static() || method.is_abstract() {
        return None;
    }
    Some((declaring_class, method))
}
