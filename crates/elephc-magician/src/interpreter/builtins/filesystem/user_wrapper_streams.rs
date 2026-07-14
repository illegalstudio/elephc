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
    let Some(bytes) = eval_user_wrapper_read_bytes(id, length, context, values)? else {
        return Ok(None);
    };
    values.string_bytes_value(&bytes).map(Some)
}

/// Dispatches `fwrite()` to a wrapper object's `stream_write()`.
pub(in crate::interpreter) fn eval_user_wrapper_fwrite_result(
    id: i64,
    data: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(written) = eval_user_wrapper_write_bytes(id, data, context, values)? else {
        return Ok(None);
    };
    values.int(written).map(Some)
}

/// Dispatches `feof()` to a wrapper object's `stream_eof()`.
pub(in crate::interpreter) fn eval_user_wrapper_feof_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    let eof = eval_user_wrapper_eof_bool(id, context, values)?;
    values.bool_value(eof).map(Some)
}

/// Dispatches `fseek()` or `rewind()` to a wrapper object's `stream_seek()`.
pub(in crate::interpreter) fn eval_user_wrapper_fseek_result(
    id: i64,
    offset: i64,
    whence: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    eval_user_wrapper_seek_bool(id, offset, whence, context, values).map(Some)
}

/// Reads the remaining or bounded contents from a userspace-wrapper stream.
pub(in crate::interpreter) fn eval_user_wrapper_stream_get_contents_result(
    id: i64,
    length: Option<usize>,
    offset: Option<i64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    let Some(bytes) =
        eval_user_wrapper_contents_bytes(id, length, offset, context, values)?
    else {
        return values.bool_value(false).map(Some);
    };
    values.string_bytes_value(&bytes).map(Some)
}

/// Copies bytes between streams when either endpoint is a userspace wrapper.
pub(in crate::interpreter) fn eval_user_wrapper_stream_copy_to_stream_result(
    from: i64,
    to: i64,
    length: Option<usize>,
    offset: Option<i64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let from_is_wrapper = context.stream_resources().user_wrapper_stream_info(from).is_some();
    let to_is_wrapper = context.stream_resources().user_wrapper_stream_info(to).is_some();
    if !from_is_wrapper && !to_is_wrapper {
        return Ok(None);
    }
    let bytes = if from_is_wrapper {
        let Some(bytes) =
            eval_user_wrapper_contents_bytes(from, length, offset, context, values)?
        else {
            return values.bool_value(false).map(Some);
        };
        bytes
    } else {
        let Some(bytes) = context
            .stream_resources_mut()
            .get_contents(from, length, offset)
        else {
            return values.bool_value(false).map(Some);
        };
        bytes
    };
    let written = if to_is_wrapper {
        let Some(written) = eval_user_wrapper_write_bytes(to, &bytes, context, values)? else {
            return values.bool_value(false).map(Some);
        };
        written
    } else {
        let Some(written) = context.stream_resources_mut().write(to, &bytes) else {
            return values.bool_value(false).map(Some);
        };
        i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?
    };
    values.int(written).map(Some)
}

/// Streams remaining wrapper bytes to eval output and returns the byte count.
pub(in crate::interpreter) fn eval_user_wrapper_fpassthru_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    let Some(bytes) = eval_user_wrapper_contents_bytes(id, None, None, context, values)? else {
        return values.bool_value(false).map(Some);
    };
    let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(len).map(Some)
}

/// Dispatches path-based filesystem probes to a wrapper object's `url_stat()`.
pub(in crate::interpreter) fn eval_user_wrapper_url_stat_result(
    path: &str,
    flags: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class_name) = context
        .stream_resources()
        .user_stream_wrapper_class_for_path(path)
    else {
        return Ok(None);
    };
    let Some(class) = context.class(&class_name).cloned() else {
        return values.bool_value(false).map(Some);
    };
    let Some((declaring_class, url_stat)) =
        eval_user_wrapper_method(class.name(), "url_stat", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let mut scope = ElephcEvalScope::new();
    let object = eval_dynamic_class_new_object(&class, Vec::new(), context, &mut scope, values)?;
    let path = values.string(path)?;
    let flags = values.int(flags)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        class.name(),
        &url_stat,
        object,
        positional_args(vec![path, flags]),
        context,
        values,
    )?;
    values.release(object)?;
    Ok(Some(result))
}

/// Reads one chunk from a userspace-wrapper stream.
pub(in crate::interpreter) fn eval_user_wrapper_read_bytes(
    id: i64,
    length: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<u8>>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_read)) =
        eval_user_wrapper_method(&info.class_name, "stream_read", context)
    else {
        return Ok(None);
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
    Ok(Some(bytes))
}

/// Writes one byte slice to a userspace-wrapper stream.
fn eval_user_wrapper_write_bytes(
    id: i64,
    data: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_write)) =
        eval_user_wrapper_method(&info.class_name, "stream_write", context)
    else {
        return Ok(None);
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
    Ok(Some(written))
}

/// Reads wrapper bytes until EOF or a finite length cap.
fn eval_user_wrapper_contents_bytes(
    id: i64,
    length: Option<usize>,
    offset: Option<i64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<u8>>, EvalStatus> {
    if let Some(offset) = offset {
        if !eval_user_wrapper_seek_bool(id, offset, 0, context, values)? {
            return Ok(None);
        }
    }
    let mut bytes = Vec::new();
    loop {
        if length.is_some_and(|limit| bytes.len() >= limit) {
            break;
        }
        if eval_user_wrapper_eof_bool(id, context, values)? {
            break;
        }
        let remaining = length
            .map(|limit| limit.saturating_sub(bytes.len()))
            .unwrap_or(8192);
        let Some(mut chunk) =
            eval_user_wrapper_read_bytes(id, remaining, context, values)?
        else {
            return Ok(None);
        };
        if chunk.is_empty() {
            break;
        }
        if let Some(limit) = length {
            chunk.truncate(limit.saturating_sub(bytes.len()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}

/// Returns a userspace-wrapper EOF result, falling back to cached EOF state.
pub(in crate::interpreter) fn eval_user_wrapper_eof_bool(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(false);
    };
    let Some((declaring_class, stream_eof)) =
        eval_user_wrapper_method(&info.class_name, "stream_eof", context)
    else {
        return Ok(info.eof);
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
    Ok(eof)
}

/// Returns a userspace-wrapper seek result, or false when the method is absent.
fn eval_user_wrapper_seek_bool(
    id: i64,
    offset: i64,
    whence: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(false);
    };
    let Some((declaring_class, stream_seek)) =
        eval_user_wrapper_method(&info.class_name, "stream_seek", context)
    else {
        return Ok(false);
    };
    let offset = values.int(offset)?;
    let whence = values.int(whence)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_seek,
        info.object,
        positional_args(vec![offset, whence]),
        context,
        values,
    )?;
    let ok = values.truthy(result)?;
    values.release(result)?;
    if ok {
        context.stream_resources_mut().set_user_wrapper_eof(id, false);
    }
    Ok(ok)
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
pub(in crate::interpreter) fn eval_user_wrapper_method(
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
