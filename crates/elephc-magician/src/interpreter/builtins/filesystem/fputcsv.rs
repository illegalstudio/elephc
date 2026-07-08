//! Purpose:
//! Declarative eval registry entry for `fputcsv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the CSV stream write helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fputcsv",
    area: Filesystem,
    params: [
        stream,
        fields,
        separator = EvalBuiltinDefaultValue::String(","),
        enclosure = EvalBuiltinDefaultValue::String("\"")
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fputcsv` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fputcsv_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fputcsv(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fputcsv` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fputcsv_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, fields] => eval_fputcsv_result(*stream, *fields, None, None, context, values),
        [stream, fields, separator] => eval_fputcsv_result(*stream, *fields, Some(*separator), None, context, values),
        [stream, fields, separator, enclosure] => eval_fputcsv_result(*stream, *fields, Some(*separator), Some(*enclosure), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
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
