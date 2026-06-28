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

/// Evaluates PHP `fopen($filename, $mode, ...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fopen(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let filename = eval_expr(&args[0], context, scope, values)?;
    let mode = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    let filename = eval_path_string(filename, values)?;
    let mode = eval_stream_string(mode, values)?;
    eval_fopen_path_result(&filename, &mode, context, scope, values)
}

/// Opens a local file stream and returns a resource cell or PHP false.
pub(in crate::interpreter) fn eval_fopen_result(
    filename: RuntimeCellHandle,
    mode: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let filename = eval_path_string(filename, values)?;
    let mode = eval_stream_string(mode, values)?;
    let mut scope = ElephcEvalScope::new();
    eval_fopen_path_result(&filename, &mode, context, &mut scope, values)
}

/// Opens a stream by already-coerced path and mode strings.
fn eval_fopen_path_result(
    filename: &str,
    mode: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) =
        eval_user_wrapper_fopen_result(filename, mode, context, scope, values)?
    {
        return Ok(result);
    }
    match context.stream_resources_mut().open_path(filename, mode) {
        Some(id) => values.resource(id),
        None => {
            values.warning("Warning: fopen(): Failed to open stream\n")?;
            values.bool_value(false)
        }
    }
}

/// Evaluates PHP `tmpfile()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_tmpfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_tmpfile_result(context, values)
}

/// Creates an anonymous temporary file stream resource or returns PHP false.
pub(in crate::interpreter) fn eval_tmpfile_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match context.stream_resources_mut().open_tmpfile() {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

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
        "fgets" => match context
            .stream_resources_mut()
            .read_line(id, usize::MAX, None, true, true)
        {
            Some(bytes) if !bytes.is_empty() => values.string_bytes_value(&bytes),
            Some(_) => values.bool_value(false),
            None => values.bool_value(false),
        },
        "feof" => {
            if let Some(result) = eval_user_wrapper_feof_result(id, context, values)? {
                return Ok(result);
            }
            values.bool_value(context.stream_resources().eof(id).unwrap_or(false))
        }
        "fflush" => values.bool_value(context.stream_resources_mut().flush(id)),
        "fpassthru" => eval_fpassthru_result(id, context, values),
        "fsync" => values.bool_value(context.stream_resources_mut().sync_all(id)),
        "fdatasync" => values.bool_value(context.stream_resources_mut().sync_data(id)),
        "ftell" => match context.stream_resources_mut().tell(id) {
            Some(position) => {
                values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)
            }
            None => values.bool_value(false),
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
                Some(metadata) => eval_stat_metadata_array(&metadata, values),
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

/// Evaluates PHP `fgetcsv($stream, $length = null, $separator = ",")`.
pub(in crate::interpreter) fn eval_builtin_fgetcsv(
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
    let separator = match args.get(2) {
        Some(separator) => Some(eval_expr(separator, context, scope, values)?),
        None => None,
    };
    eval_fgetcsv_result(stream, length, separator, context, values)
}

/// Reads and parses one CSV record from a materialized stream resource.
pub(in crate::interpreter) fn eval_fgetcsv_result(
    stream: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    separator: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let length = eval_optional_stream_length(length, values)?.unwrap_or(usize::MAX);
    let separator = eval_optional_delimiter(separator, b',', values)?;
    let Some(mut line) = context
        .stream_resources_mut()
        .read_line(id, length, None, true, true)
    else {
        return values.bool_value(false);
    };
    if line.is_empty() {
        return values.bool_value(false);
    }
    eval_trim_csv_line_end(&mut line);
    let fields = eval_parse_csv_record(&line, separator, b'"');
    eval_csv_fields_array(&fields, values)
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

/// Evaluates PHP `fputcsv($stream, $fields, $separator = ",", $enclosure = "\"")`.
pub(in crate::interpreter) fn eval_builtin_fputcsv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let fields = eval_expr(&args[1], context, scope, values)?;
    let separator = match args.get(2) {
        Some(separator) => Some(eval_expr(separator, context, scope, values)?),
        None => None,
    };
    let enclosure = match args.get(3) {
        Some(enclosure) => Some(eval_expr(enclosure, context, scope, values)?),
        None => None,
    };
    eval_fputcsv_result(stream, fields, separator, enclosure, context, values)
}

/// Formats and writes one CSV record to a materialized stream resource.
pub(in crate::interpreter) fn eval_fputcsv_result(
    stream: RuntimeCellHandle,
    fields: RuntimeCellHandle,
    separator: Option<RuntimeCellHandle>,
    enclosure: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let separator = eval_optional_delimiter(separator, b',', values)?;
    let enclosure = eval_optional_delimiter(enclosure, b'"', values)?;
    let output = eval_format_csv_record(fields, separator, enclosure, values)?;
    match context.stream_resources_mut().write(id, &output) {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `fprintf($stream, $format, ...$values)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let format = eval_expr(&args[1], context, scope, values)?;
    let mut format_args = Vec::with_capacity(args.len().saturating_sub(2));
    for arg in &args[2..] {
        format_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_fprintf_result(stream, format, &format_args, context, values)
}

/// Evaluates PHP `fscanf($stream, $format, ...$vars)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fscanf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let format = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    eval_fscanf_result(stream, format, context, values)
}

/// Reads one line from a stream and scans it with the eval `sscanf()` subset.
pub(in crate::interpreter) fn eval_fscanf_result(
    stream: RuntimeCellHandle,
    format: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let Some(line) = context
        .stream_resources_mut()
        .read_line(id, usize::MAX, None, true, true)
    else {
        return values.bool_value(false);
    };
    let input = values.string_bytes_value(&line)?;
    eval_sscanf_result(input, format, values)
}

/// Formats and writes `fprintf()` arguments to a materialized stream resource.
pub(in crate::interpreter) fn eval_fprintf_result(
    stream: RuntimeCellHandle,
    format: RuntimeCellHandle,
    format_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let format = values.string_bytes(format)?;
    let output = eval_sprintf_bytes(&format, format_args, values)?;
    match context.stream_resources_mut().write(id, &output) {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `flock($stream, $operation, &$would_block = null)` over eval call args.
pub(in crate::interpreter) fn eval_builtin_flock(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (stream, operation, would_block_target) =
        eval_flock_direct_args(args, context, scope, values)?;
    let (success, would_block) = eval_flock_result(stream, operation, context, values)?;
    if let Some(target) = would_block_target {
        let value = values.bool_value(would_block)?;
        eval_write_direct_ref_target(
            &target,
            value,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        )?;
    }
    values.bool_value(success)
}

/// Applies a materialized PHP `flock()` operation to a local eval stream resource.
pub(in crate::interpreter) fn eval_flock_result(
    stream: RuntimeCellHandle,
    operation: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(bool, bool), EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let operation = eval_int_value(operation, values)?;
    Ok(context
        .stream_resources()
        .flock(id, operation)
        .unwrap_or((false, false)))
}

/// Evaluates and binds direct `flock()` arguments while keeping by-ref output writable.
fn eval_flock_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<
    (
        RuntimeCellHandle,
        RuntimeCellHandle,
        Option<EvalReferenceTarget>,
    ),
    EvalStatus,
> {
    let mut stream = None;
    let mut operation = None;
    let mut would_block = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "stream",
                1 => "operation",
                2 => "would_block",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "stream" => {
                if stream.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                stream = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "operation" => {
                if operation.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                operation = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "would_block" => {
                if would_block.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let (_, target) = eval_call_arg_value(arg.value(), context, scope, values)?;
                would_block = Some(target.ok_or(EvalStatus::RuntimeFatal)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let stream = stream.ok_or(EvalStatus::RuntimeFatal)?;
    let operation = operation.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((stream, operation, would_block))
}

/// Evaluates PHP `vfprintf($stream, $format, $values)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_vfprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, format, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let format = eval_expr(format, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_vfprintf_result(stream, format, array, context, values)
}

/// Formats and writes `vfprintf()` array arguments to a materialized stream resource.
pub(in crate::interpreter) fn eval_vfprintf_result(
    stream: RuntimeCellHandle,
    format: RuntimeCellHandle,
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format_args = eval_sprintf_argument_array_values(array, values)?;
    eval_fprintf_result(stream, format, &format_args, context, values)
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

/// Builds PHP's stream metadata array for one eval-local stream resource.
fn eval_stream_get_meta_data_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(meta) = context.stream_resources().meta_data(id) else {
        return values.bool_value(false);
    };
    let mut result = values.assoc_new(9)?;
    result = eval_stream_meta_set_bool(result, "timed_out", false, values)?;
    result = eval_stream_meta_set_bool(result, "blocked", true, values)?;
    result = eval_stream_meta_set_bool(result, "eof", meta.eof, values)?;
    result = eval_stream_meta_set_string(result, "wrapper_type", "plainfile", values)?;
    result = eval_stream_meta_set_string(result, "stream_type", "STDIO", values)?;
    result = eval_stream_meta_set_string(result, "mode", &meta.mode, values)?;
    result = eval_stream_meta_set_int(result, "unread_bytes", 0, values)?;
    result = eval_stream_meta_set_bool(result, "seekable", true, values)?;
    eval_stream_meta_set_string(result, "uri", &meta.uri, values)
}

/// Converts a runtime resource cell into eval's zero-based stream id.
fn eval_stream_resource_id(
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(stream)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(stream, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}

/// Converts a stream length argument into a non-negative `usize`.
fn eval_nonnegative_usize(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<usize, EvalStatus> {
    let value = eval_int_value(value, values)?;
    usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Converts an optional stream length where null and -1 mean "read all".
fn eval_optional_stream_length(
    value: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let Some(value) = value else {
        return Ok(None);
    };
    if values.type_tag(value)? == EVAL_TAG_NULL {
        return Ok(None);
    }
    let value = eval_int_value(value, values)?;
    if value == -1 {
        return Ok(None);
    }
    Ok(Some(
        usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)?,
    ))
}

/// Converts an optional absolute stream offset where null and -1 mean no seek.
fn eval_optional_stream_offset(
    value: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, EvalStatus> {
    let Some(value) = value else {
        return Ok(None);
    };
    if values.type_tag(value)? == EVAL_TAG_NULL {
        return Ok(None);
    }
    let value = eval_int_value(value, values)?;
    if value < 0 {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Converts one runtime cell to a UTF-8 string for stream mode arguments.
fn eval_stream_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Converts an optional one-byte delimiter argument to a byte value.
fn eval_optional_delimiter(
    value: Option<RuntimeCellHandle>,
    default: u8,
    values: &mut impl RuntimeValueOps,
) -> Result<u8, EvalStatus> {
    let Some(value) = value else {
        return Ok(default);
    };
    if values.type_tag(value)? == EVAL_TAG_NULL {
        return Ok(default);
    }
    Ok(values.string_bytes(value)?.first().copied().unwrap_or(default))
}

/// Removes CR/LF line terminators from a CSV record buffer.
fn eval_trim_csv_line_end(line: &mut Vec<u8>) {
    if line.ends_with(b"\n") {
        line.pop();
    }
    if line.ends_with(b"\r") {
        line.pop();
    }
}

/// Parses one CSV record using PHP-style doubled-enclosure escaping.
fn eval_parse_csv_record(line: &[u8], separator: u8, enclosure: u8) -> Vec<Vec<u8>> {
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut quoted = false;
    let mut index = 0;
    while index < line.len() {
        let byte = line[index];
        if quoted {
            if byte == enclosure {
                if line.get(index + 1).copied() == Some(enclosure) {
                    field.push(enclosure);
                    index += 2;
                    continue;
                }
                quoted = false;
            } else {
                field.push(byte);
            }
        } else if byte == enclosure && field.is_empty() {
            quoted = true;
        } else if byte == separator {
            fields.push(std::mem::take(&mut field));
        } else {
            field.push(byte);
        }
        index += 1;
    }
    fields.push(field);
    fields
}

/// Builds a PHP indexed array from parsed CSV field bytes.
fn eval_csv_fields_array(
    fields: &[Vec<u8>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(fields.len())?;
    for (index, field) in fields.iter().enumerate() {
        result = eval_array_set_indexed_bytes(result, index, field, values)?;
    }
    Ok(result)
}

/// Formats one PHP array-like value as a CSV record ending in LF.
fn eval_format_csv_record(
    fields: RuntimeCellHandle,
    separator: u8,
    enclosure: u8,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    if !values.is_array_like(fields)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(fields)?;
    let mut output = Vec::new();
    for position in 0..len {
        if position > 0 {
            output.push(separator);
        }
        let key = values.array_iter_key(fields, position)?;
        let value = values.array_get(fields, key)?;
        let bytes = values.string_bytes(value)?;
        eval_append_csv_field(&mut output, &bytes, separator, enclosure);
    }
    output.push(b'\n');
    Ok(output)
}

/// Appends one CSV field, quoting and escaping only when required.
fn eval_append_csv_field(output: &mut Vec<u8>, field: &[u8], separator: u8, enclosure: u8) {
    let needs_quotes = field
        .iter()
        .any(|byte| matches!(*byte, b'\n' | b'\r') || *byte == separator || *byte == enclosure);
    if !needs_quotes {
        output.extend_from_slice(field);
        return;
    }
    output.push(enclosure);
    for byte in field {
        if *byte == enclosure {
            output.push(enclosure);
        }
        output.push(*byte);
    }
    output.push(enclosure);
}

/// Inserts a boolean field into the stream metadata array.
fn eval_stream_meta_set_bool(
    array: RuntimeCellHandle,
    key: &str,
    value: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.bool_value(value)?;
    values.array_set(array, key, value)
}

/// Inserts an integer field into the stream metadata array.
fn eval_stream_meta_set_int(
    array: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Inserts a string field into the stream metadata array.
fn eval_stream_meta_set_string(
    array: RuntimeCellHandle,
    key: &str,
    value: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.string(value)?;
    values.array_set(array, key, value)
}
