//! Purpose:
//! Declarative eval registry entry for `fputcsv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the CSV stream write helper.

eval_builtin! {
    contract: "fputcsv",
    area: Filesystem,
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
        [stream, fields, rest @ ..] if rest.len() <= 4 => {
            eval_fputcsv_result(*stream, *fields, rest, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fputcsv($stream, $fields, $separator, $enclosure, $escape, $eol)`.
pub(in crate::interpreter) fn eval_builtin_fputcsv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=6).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let fields = eval_expr(&args[1], context, scope, values)?;
    let mut rest = Vec::with_capacity(args.len() - 2);
    for arg in &args[2..] {
        rest.push(eval_expr(arg, context, scope, values)?);
    }
    eval_fputcsv_result(stream, fields, &rest, context, values)
}

/// Formats and writes one CSV record to a materialized stream resource.
///
/// `rest` holds the evaluated `$separator`, `$enclosure`, `$escape` and `$eol`, as far as the
/// call passed them; an omitted `$escape` raises php 8.4's deprecation before anything is written.
pub(in crate::interpreter) fn eval_fputcsv_result(
    stream: RuntimeCellHandle,
    fields: RuntimeCellHandle,
    rest: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    if !values.is_array_like(fields)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let separator = eval_csv_control_byte(
        rest.first().copied(),
        b',',
        CsvControlArgument {
            function: "fputcsv",
            position: 3,
            parameter: "separator",
            empty_allowed: false,
        },
        context,
        values,
    )?;
    let enclosure = eval_csv_control_byte(
        rest.get(1).copied(),
        b'"',
        CsvControlArgument {
            function: "fputcsv",
            position: 4,
            parameter: "enclosure",
            empty_allowed: false,
        },
        context,
        values,
    )?;
    let escape = eval_csv_escape_argument(rest.get(2).copied(), "fputcsv", 5, context, values)?;
    let eol = match rest.get(3).copied() {
        Some(eol) if values.type_tag(eol)? != EVAL_TAG_NULL => Some(values.string_bytes(eol)?),
        _ => None,
    };
    let len = values.array_len(fields)?;
    let mut field_bytes = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(fields, position)?;
        let value = values.array_get(fields, key)?;
        field_bytes.push(values.string_bytes(value)?);
    }
    let output = eval_php_fputcsv(&field_bytes, separator, enclosure, escape, eol.as_deref());
    match context.stream_resources_mut().write(id, &output) {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}
