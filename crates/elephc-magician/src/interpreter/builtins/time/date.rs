//! Purpose:
//! Eval registry entry and implementation for `date` plus shared date-format helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - `gmdate` calls this file for shared formatting and UTC/local timestamp conversion.

use super::super::*;
use super::*;

eval_builtin! {
    contract: "date",
    area: Time,
    direct: Time,
    values: Time,
}

/// Evaluates PHP `date($format, $timestamp = time())` for the eval subset.
pub(in crate::interpreter) fn eval_builtin_date(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_date_like("date", args, context, scope, values)
}

/// Evaluates PHP `date($format, $timestamp = time())` for the eval subset.
pub(in crate::interpreter) fn eval_builtin_date_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [format] => {
            let format = eval_expr(format, context, scope, values)?;
            eval_date_result(name, format, None, context, values)
        }
        [format, timestamp] => {
            let format = eval_expr(format, context, scope, values)?;
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            eval_date_result(name, format, Some(timestamp), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one Unix timestamp through PHP `date()` token rules supported by elephc.
pub(in crate::interpreter) fn eval_date_result(
    name: &str,
    format: RuntimeCellHandle,
    timestamp: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format = values.string_bytes(format)?;
    let timestamp = match timestamp {
        Some(timestamp) if !values.is_null(timestamp)? => eval_int_value(timestamp, values)?,
        None => eval_current_unix_timestamp()?,
        Some(_) => eval_current_unix_timestamp()?,
    };
    let (timezone, localtime) = match name {
        "date" => (eval_request_timezone(context, values)?, true),
        "gmdate" => ("UTC".to_owned(), false),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let output = elephc_tz::format_timestamp_php(timestamp, &timezone, &format, localtime)
        .ok_or(EvalStatus::RuntimeFatal)?;
    values.string_bytes_value(&output)
}

/// Wide broken-down date fields derived from vendored timelib, not libc's c_int year.
#[derive(Debug, Clone, Copy)]
pub(in crate::interpreter) struct EvalBrokenDownTime {
    pub tm_sec: i64,
    pub tm_min: i64,
    pub tm_hour: i64,
    pub tm_mday: i64,
    pub tm_mon: i64,
    pub tm_year: i64,
    pub tm_wday: i64,
    pub tm_yday: i64,
    pub tm_isdst: i64,
}

/// Decomposes one timestamp with the authoritative request timezone and frozen timelib data.
pub(in crate::interpreter) fn eval_context_localtime(
    timestamp: i64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalBrokenDownTime, EvalStatus> {
    let timezone = eval_request_timezone(context, values)?;
    eval_timezone_broken_down_time(timestamp, &timezone)
}

/// Decomposes a timestamp in an explicit zone without reading the clock or request state.
pub(in crate::interpreter) fn eval_timezone_broken_down_time(
    timestamp: i64,
    timezone: &str,
) -> Result<EvalBrokenDownTime, EvalStatus> {
    let bytes = elephc_tz::format_timestamp_php(
        timestamp, timezone, b"Y\tn\tj\tG\ti\ts\tw\tz\tI", true,
    ).ok_or(EvalStatus::RuntimeFatal)?;
    let fields = bytes.split(|byte| *byte == b'\t').map(|field| {
        std::str::from_utf8(field).map_err(|_| EvalStatus::RuntimeFatal)?
            .parse::<i64>().map_err(|_| EvalStatus::RuntimeFatal)
    }).collect::<Result<Vec<_>, _>>()?;
    let [year, month, day, hour, minute, second, weekday, yearday, isdst]: [i64; 9] =
        fields.try_into().map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(EvalBrokenDownTime {
        tm_sec: second, tm_min: minute, tm_hour: hour, tm_mday: day,
        tm_mon: month - 1, tm_year: year - 1900, tm_wday: weekday,
        tm_yday: yearday, tm_isdst: isdst,
    })
}

/// Returns a checked month index for PHP's English date-name table.
pub(in crate::interpreter) fn eval_tm_month_index(tm: &EvalBrokenDownTime) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_mon).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_MONTH_NAMES.len() { return Err(EvalStatus::RuntimeFatal); }
    Ok(index)
}

/// Returns a checked weekday index for PHP's English date-name table.
pub(in crate::interpreter) fn eval_tm_weekday_index(tm: &EvalBrokenDownTime) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_wday).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_WEEKDAY_NAMES.len() { return Err(EvalStatus::RuntimeFatal); }
    Ok(index)
}
