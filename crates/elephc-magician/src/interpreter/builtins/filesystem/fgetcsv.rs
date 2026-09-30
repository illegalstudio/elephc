//! Purpose:
//! Declarative eval registry entry for `fgetcsv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the CSV stream read helper.

eval_builtin! {
    contract: "fgetcsv",
    area: Filesystem,
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
        [stream, rest @ ..] if rest.len() <= 4 => {
            eval_fgetcsv_result(*stream, rest, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fgetcsv($stream, $length = null, $separator = ",", $enclosure = "\"", $escape = "\\")`.
pub(in crate::interpreter) fn eval_builtin_fgetcsv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=5).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let mut rest = Vec::with_capacity(args.len() - 1);
    for arg in &args[1..] {
        rest.push(eval_expr(arg, context, scope, values)?);
    }
    eval_fgetcsv_result(stream, &rest, context, values)
}

/// Reads and parses one CSV record from a materialized stream resource.
///
/// `rest` holds the evaluated `$length`, `$separator`, `$enclosure` and `$escape`, in order and
/// as far as the call passed them. php validates the three controls — with the `$escape`
/// deprecation when it is omitted — and THEN the length: `$length` of `null` or `0` reads the
/// whole line, a negative one is a `ValueError`, and a positive one bounds the FIRST physical
/// line only; the lines a quoted field continues onto are read whole.
pub(in crate::interpreter) fn eval_fgetcsv_result(
    stream: RuntimeCellHandle,
    rest: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let separator = eval_csv_control_byte(
        rest.get(1).copied(),
        b',',
        CsvControlArgument {
            function: "fgetcsv",
            position: 3,
            parameter: "separator",
            empty_allowed: false,
        },
        context,
        values,
    )?;
    let enclosure = eval_csv_control_byte(
        rest.get(2).copied(),
        b'"',
        CsvControlArgument {
            function: "fgetcsv",
            position: 4,
            parameter: "enclosure",
            empty_allowed: false,
        },
        context,
        values,
    )?;
    let escape = eval_csv_escape_argument(rest.get(3).copied(), "fgetcsv", 5, context, values)?;
    let length = match rest.first().copied() {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => eval_int_value(length, values)?,
        _ => 0,
    };
    if !(0..i64::MAX).contains(&length) {
        return eval_csv_raise_value_error(
            "fgetcsv(): Argument #2 ($length) must be between 0 and 9223372036854775806",
            context,
            values,
        );
    }
    let bound = if length == 0 { usize::MAX } else { usize::try_from(length).unwrap_or(usize::MAX) };
    let streams = context.stream_resources_mut();
    let Some(line) = streams.read_line(id, bound, None, true, true) else {
        return values.bool_value(false);
    };
    if line.is_empty() {
        return values.bool_value(false);
    }
    let record = eval_php_fgetcsv(line, separator, enclosure, escape, || {
        streams
            .read_line(id, usize::MAX, None, true, true)
            .filter(|line| !line.is_empty())
    });
    eval_csv_record_array(record, values)
}
