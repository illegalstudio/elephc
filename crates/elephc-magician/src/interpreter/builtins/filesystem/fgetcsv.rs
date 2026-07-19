//! Purpose:
//! Declarative eval registry entry for `fgetcsv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the CSV stream read helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fgetcsv",
    area: Filesystem,
    params: [
        stream,
        length = EvalBuiltinDefaultValue::Null,
        separator = EvalBuiltinDefaultValue::String(",")
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fgetcsv` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fgetcsv_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fgetcsv(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fgetcsv` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fgetcsv_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_fgetcsv_result(*stream, None, None, context, values),
        [stream, length] => eval_fgetcsv_result(*stream, Some(*length), None, context, values),
        [stream, length, separator] => eval_fgetcsv_result(*stream, Some(*length), Some(*separator), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
        result = super::scandir::eval_array_set_indexed_bytes(result, index, field, values)?;
    }
    Ok(result)
}
