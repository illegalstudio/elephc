//! Purpose:
//! Implements eval-local file stream builtins backed by host file handles.
//! These builtins turn PHP resource cells into ids stored in the eval context's
//! stream table.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Runtime resource payloads are zero-based; `get_resource_id()` exposes payload + 1.
//! - File-backed streams stay in `EvalStreamResources`; userspace wrapper calls
//!   delegate to the focused wrapper-dispatch helper module.

use super::super::super::*;
use super::*;

mod common;
mod csv_format;
mod flock;
mod metadata;

pub(in crate::interpreter) use common::*;
pub(in crate::interpreter) use csv_format::*;
pub(in crate::interpreter) use flock::*;
pub(in crate::interpreter) use metadata::*;

/// Evaluates one unary stream builtin over an eval expression.
pub(in crate::interpreter) fn eval_builtin_unary_stream(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_unary_stream_result(name, stream, context, values)
}

/// Evaluates a materialized unary stream builtin argument.
pub(in crate::interpreter) fn eval_unary_stream_result(
    name: &str,
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    match name {
        "fclose" => {
            if let Some(result) = eval_user_wrapper_fclose_result(id, context, values)? {
                return Ok(result);
            }
            values.bool_value(context.stream_resources_mut().close(id))
        }
        "fgetc" => {
            if let Some(result) = eval_user_wrapper_fread_result(id, 1, context, values)? {
                return Ok(result);
            }
            match context.stream_resources_mut().read(id, 1) {
                Some(bytes) if !bytes.is_empty() => values.string_bytes_value(&bytes),
                Some(_) => values.bool_value(false),
                None => values.bool_value(false),
            }
        }
        "fgets" => {
            if let Some(result) = eval_user_wrapper_fgets_result(id, context, values)? {
                return Ok(result);
            }
            match context
                .stream_resources_mut()
                .read_line(id, usize::MAX, None, true, true)
            {
                Some(bytes) if !bytes.is_empty() => values.string_bytes_value(&bytes),
                Some(_) => values.bool_value(false),
                None => values.bool_value(false),
            }
        }
        "feof" => {
            if let Some(result) = eval_user_wrapper_feof_result(id, context, values)? {
                return Ok(result);
            }
            values.bool_value(context.stream_resources().eof(id).unwrap_or(false))
        }
        "fflush" => {
            if let Some(result) = eval_user_wrapper_fflush_result(id, context, values)? {
                return Ok(result);
            }
            values.bool_value(context.stream_resources_mut().flush(id))
        }
        "fpassthru" => eval_fpassthru_result(id, context, values),
        "fsync" => values.bool_value(context.stream_resources_mut().sync_all(id)),
        "fdatasync" => values.bool_value(context.stream_resources_mut().sync_data(id)),
        "ftell" => {
            if let Some(result) = eval_user_wrapper_ftell_result(id, context, values)? {
                return Ok(result);
            }
            match context.stream_resources_mut().tell(id) {
                Some(position) => {
                    values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)
                }
                None => values.bool_value(false),
            }
        },
        "rewind" => {
            if let Some(seek_ok) = eval_user_wrapper_fseek_result(id, 0, 0, context, values)? {
                return values.bool_value(seek_ok);
            }
            values.bool_value(context.stream_resources_mut().rewind(id))
        }
        "fstat" => {
            if let Some(result) = eval_user_wrapper_fstat_result(id, context, values)? {
                return Ok(result);
            }
            match context.stream_resources().metadata(id) {
                Some(metadata) => super::stat::eval_stat_metadata_array(&metadata, values),
                None => values.bool_value(false),
            }
        }
        "stream_get_meta_data" => eval_stream_get_meta_data_result(id, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Streams all remaining bytes to eval output and returns the emitted byte count.
fn eval_fpassthru_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_user_wrapper_fpassthru_result(id, context, values)? {
        return Ok(result);
    }
    let Some(bytes) = context.stream_resources_mut().get_contents(id, None, None) else {
        return values.bool_value(false);
    };
    let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(len)
}

/// Evaluates PHP `fread($stream, $length)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fread(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_fread_result(stream, length, context, values)
}

/// Reads bytes from a materialized stream resource.
pub(in crate::interpreter) fn eval_fread_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let length = eval_nonnegative_usize(length, values)?;
    if let Some(result) = eval_user_wrapper_fread_result(id, length, context, values)? {
        return Ok(result);
    }
    match context.stream_resources_mut().read(id, length) {
        Some(bytes) => values.string_bytes_value(&bytes),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `fwrite($stream, $data)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fwrite(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, data] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    eval_fwrite_result(stream, data, context, values)
}

/// Writes bytes to a materialized stream resource.
pub(in crate::interpreter) fn eval_fwrite_result(
    stream: RuntimeCellHandle,
    data: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let data = values.string_bytes(data)?;
    if let Some(result) = eval_user_wrapper_fwrite_result(id, &data, context, values)? {
        return Ok(result);
    }
    match context.stream_resources_mut().write(id, &data) {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `fseek($stream, $offset, $whence = SEEK_SET)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fseek(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let offset = eval_expr(&args[1], context, scope, values)?;
    let whence = match args.get(2) {
        Some(whence) => Some(eval_expr(whence, context, scope, values)?),
        None => None,
    };
    eval_fseek_result(stream, offset, whence, context, values)
}

/// Seeks a materialized stream and returns PHP's 0 or -1 status code.
pub(in crate::interpreter) fn eval_fseek_result(
    stream: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    whence: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let offset = eval_int_value(offset, values)?;
    let whence = match whence {
        Some(whence) => eval_int_value(whence, values)?,
        None => 0,
    };
    if let Some(seek_ok) = eval_user_wrapper_fseek_result(id, offset, whence, context, values)? {
        return values.int(if seek_ok { 0 } else { -1 });
    }
    let status = if context.stream_resources_mut().seek(id, offset, whence) {
        0
    } else {
        -1
    };
    values.int(status)
}

/// Evaluates PHP `ftruncate($stream, $size)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_ftruncate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, size] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let size = eval_expr(size, context, scope, values)?;
    eval_ftruncate_result(stream, size, context, values)
}

/// Truncates a materialized stream resource.
pub(in crate::interpreter) fn eval_ftruncate_result(
    stream: RuntimeCellHandle,
    size: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let size = eval_int_value(size, values)?;
    let Ok(size) = u64::try_from(size) else {
        return values.bool_value(false);
    };
    if let Some(result) = eval_user_wrapper_ftruncate_result(id, size, context, values)? {
        return Ok(result);
    }
    values.bool_value(context.stream_resources_mut().truncate(id, size))
}

/// Evaluates PHP `stream_get_contents($stream, $length = null, $offset = -1)`.
pub(in crate::interpreter) fn eval_builtin_stream_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let length = match args.get(1) {
        Some(length) => Some(eval_expr(length, context, scope, values)?),
        None => None,
    };
    let offset = match args.get(2) {
        Some(offset) => Some(eval_expr(offset, context, scope, values)?),
        None => None,
    };
    eval_stream_get_contents_result(stream, length, offset, context, values)
}

/// Reads the remaining or bounded contents from a materialized stream resource.
pub(in crate::interpreter) fn eval_stream_get_contents_result(
    stream: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    offset: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let length = eval_optional_stream_length(length, values)?;
    let offset = eval_optional_stream_offset(offset, values)?;
    if let Some(result) =
        eval_user_wrapper_stream_get_contents_result(id, length, offset, context, values)?
    {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .get_contents(id, length, offset)
    {
        Some(bytes) => values.string_bytes_value(&bytes),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `stream_copy_to_stream($from, $to, $length = null, $offset = -1)`.
pub(in crate::interpreter) fn eval_builtin_stream_copy_to_stream(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let from = eval_expr(&args[0], context, scope, values)?;
    let to = eval_expr(&args[1], context, scope, values)?;
    let length = match args.get(2) {
        Some(length) => Some(eval_expr(length, context, scope, values)?),
        None => None,
    };
    let offset = match args.get(3) {
        Some(offset) => Some(eval_expr(offset, context, scope, values)?),
        None => None,
    };
    eval_stream_copy_to_stream_result(from, to, length, offset, context, values)
}

/// Evaluates PHP `stream_get_line($stream, $length, $ending = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_get_line(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let length = eval_expr(&args[1], context, scope, values)?;
    let ending = match args.get(2) {
        Some(ending) => Some(eval_expr(ending, context, scope, values)?),
        None => None,
    };
    eval_stream_get_line_result(stream, length, ending, context, values)
}

/// Reads one line-like byte sequence from a materialized stream resource.
pub(in crate::interpreter) fn eval_stream_get_line_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    ending: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let length = eval_nonnegative_usize(length, values)?;
    let ending = match ending {
        Some(ending) if values.type_tag(ending)? != EVAL_TAG_NULL => {
            Some(values.string_bytes(ending)?)
        }
        _ => None,
    };
    if let Some(result) =
        eval_user_wrapper_stream_get_line_result(id, length, ending.as_deref(), context, values)?
    {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .read_line(id, length, ending.as_deref(), false, false)
    {
        Some(bytes) if !bytes.is_empty() => values.string_bytes_value(&bytes),
        Some(_) => values.bool_value(false),
        None => values.bool_value(false),
    }
}

/// Copies bytes between two materialized stream resources.
pub(in crate::interpreter) fn eval_stream_copy_to_stream_result(
    from: RuntimeCellHandle,
    to: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    offset: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let from = eval_stream_resource_id(from, values)?;
    let to = eval_stream_resource_id(to, values)?;
    let length = eval_optional_stream_length(length, values)?;
    let offset = eval_optional_stream_offset(offset, values)?;
    if let Some(result) =
        eval_user_wrapper_stream_copy_to_stream_result(from, to, length, offset, context, values)?
    {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .copy_to_stream(from, to, length, offset)
    {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}
