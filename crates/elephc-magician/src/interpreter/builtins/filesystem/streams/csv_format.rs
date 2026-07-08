//! Purpose:
//! CSV and printf-family stream builtins for eval-local file resources.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::streams` re-exports.
//! - Filesystem stream declaration dispatchers.
//!
//! Key details:
//! - CSV parsing implements the small PHP-compatible subset used by eval tests.
//! - Formatting delegates to the shared `sprintf` byte formatter.

use super::*;

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
        result = super::super::scandir::eval_array_set_indexed_bytes(result, index, field, values)?;
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
