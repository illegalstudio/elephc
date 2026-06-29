//! Purpose:
//! Implements calendar decomposition helpers such as `checkdate()`, `getdate()`,
//! `localtime()`, and `hrtime()` for eval builtin execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` re-exports.
//!
//! Key details:
//! - Local calendar decomposition uses the eval context timezone, which defaults
//!   to UTC to match elephc's native runtime initialization.

use super::super::super::*;
use super::super::*;
use super::*;

const EVAL_LOCALTIME_KEYS: &[&str; 9] = &[
    "tm_sec", "tm_min", "tm_hour", "tm_mday", "tm_mon", "tm_year", "tm_wday", "tm_yday",
    "tm_isdst",
];

/// Evaluates PHP `checkdate(month, day, year)` over three eval expressions.
pub(in crate::interpreter) fn eval_builtin_checkdate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [month, day, year] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let month = eval_expr(month, context, scope, values)?;
    let day = eval_expr(day, context, scope, values)?;
    let year = eval_expr(year, context, scope, values)?;
    eval_checkdate_result(month, day, year, values)
}

/// Returns whether the supplied month/day/year tuple is a valid Gregorian date.
pub(in crate::interpreter) fn eval_checkdate_result(
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let month = eval_int_value(month, values)?;
    let day = eval_int_value(day, values)?;
    let year = eval_int_value(year, values)?;
    values.bool_value(eval_checkdate_parts(month, day, year))
}

/// Tests PHP `checkdate()` bounds and leap-year behavior for integer components.
fn eval_checkdate_parts(month: i64, day: i64, year: i64) -> bool {
    if !(1..=12).contains(&month) || !(1..=32767).contains(&year) {
        return false;
    }
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if eval_is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// Returns whether one Gregorian year is a leap year.
fn eval_is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Evaluates PHP `getdate($timestamp = null)`.
pub(in crate::interpreter) fn eval_builtin_getdate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_getdate_result(None, context, values),
        [timestamp] => {
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            eval_getdate_result(Some(timestamp), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds PHP's `getdate()` associative array for one optional timestamp.
pub(in crate::interpreter) fn eval_getdate_result(
    timestamp: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = eval_optional_timestamp(timestamp, values)?;
    let tm = eval_context_localtime(timestamp, context)?;
    let mut result = values.assoc_new(11)?;
    result = eval_array_set_string_int(result, "seconds", i64::from(tm.tm_sec), values)?;
    result = eval_array_set_string_int(result, "minutes", i64::from(tm.tm_min), values)?;
    result = eval_array_set_string_int(result, "hours", i64::from(tm.tm_hour), values)?;
    result = eval_array_set_string_int(result, "mday", i64::from(tm.tm_mday), values)?;
    result = eval_array_set_string_int(result, "wday", i64::from(tm.tm_wday), values)?;
    result = eval_array_set_string_int(result, "mon", i64::from(tm.tm_mon + 1), values)?;
    result = eval_array_set_string_int(result, "year", i64::from(tm.tm_year + 1900), values)?;
    result = eval_array_set_string_int(result, "yday", i64::from(tm.tm_yday), values)?;
    result = eval_array_set_string_str(
        result,
        "weekday",
        EVAL_WEEKDAY_NAMES[eval_tm_weekday_index(&tm)?],
        values,
    )?;
    result = eval_array_set_string_str(
        result,
        "month",
        EVAL_MONTH_NAMES[eval_tm_month_index(&tm)?],
        values,
    )?;
    eval_array_set_int_int(result, 0, timestamp, values)
}

/// Evaluates PHP `localtime($timestamp = null, $associative = false)`.
pub(in crate::interpreter) fn eval_builtin_localtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_localtime_result(None, None, context, values),
        [timestamp] => {
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            eval_localtime_result(Some(timestamp), None, context, values)
        }
        [timestamp, associative] => {
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            let associative = eval_expr(associative, context, scope, values)?;
            eval_localtime_result(Some(timestamp), Some(associative), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds PHP's `localtime()` array for one optional timestamp and key mode.
pub(in crate::interpreter) fn eval_localtime_result(
    timestamp: Option<RuntimeCellHandle>,
    associative: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = eval_optional_timestamp(timestamp, values)?;
    let associative = associative
        .map(|value| values.truthy(value))
        .transpose()?
        .unwrap_or(false);
    let tm = eval_context_localtime(timestamp, context)?;
    let fields = [
        tm.tm_sec,
        tm.tm_min,
        tm.tm_hour,
        tm.tm_mday,
        tm.tm_mon,
        tm.tm_year,
        tm.tm_wday,
        tm.tm_yday,
        tm.tm_isdst,
    ];
    if associative {
        let mut result = values.assoc_new(fields.len())?;
        for (key, value) in EVAL_LOCALTIME_KEYS.iter().zip(fields) {
            result = eval_array_set_string_int(result, key, i64::from(value), values)?;
        }
        return Ok(result);
    }
    let mut result = values.array_new(fields.len())?;
    for (index, value) in fields.into_iter().enumerate() {
        result = eval_array_set_int_int(result, index as i64, i64::from(value), values)?;
    }
    Ok(result)
}

/// Evaluates PHP `hrtime($as_number = false)`.
pub(in crate::interpreter) fn eval_builtin_hrtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_hrtime_result(None, values),
        [as_number] => {
            let as_number = eval_expr(as_number, context, scope, values)?;
            eval_hrtime_result(Some(as_number), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns monotonic time as either nanoseconds or `[seconds, nanoseconds]`.
pub(in crate::interpreter) fn eval_hrtime_result(
    as_number: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (seconds, nanoseconds) = eval_monotonic_time()?;
    let as_number = as_number
        .map(|value| values.truthy(value))
        .transpose()?
        .unwrap_or(false);
    if as_number {
        let total = seconds
            .checked_mul(1_000_000_000)
            .and_then(|value| value.checked_add(nanoseconds))
            .ok_or(EvalStatus::RuntimeFatal)?;
        return values.int(total);
    }
    let mut result = values.array_new(2)?;
    result = eval_array_set_int_int(result, 0, seconds, values)?;
    eval_array_set_int_int(result, 1, nanoseconds, values)
}

/// Reads the monotonic clock in whole seconds and nanoseconds.
fn eval_monotonic_time() -> Result<(i64, i64), EvalStatus> {
    let mut timespec = MaybeUninit::<libc::timespec>::uninit();
    let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, timespec.as_mut_ptr()) };
    if status != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let timespec = unsafe { timespec.assume_init() };
    Ok((timespec.tv_sec, timespec.tv_nsec))
}

/// Coerces an optional timestamp argument, treating null/omitted as the current time.
fn eval_optional_timestamp(
    timestamp: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    match timestamp {
        Some(timestamp) if !values.is_null(timestamp)? => eval_int_value(timestamp, values),
        _ => eval_current_unix_timestamp(),
    }
}

/// Writes one string-keyed integer entry into a PHP array.
fn eval_array_set_string_int(
    array: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Writes one string-keyed string entry into a PHP array.
fn eval_array_set_string_str(
    array: RuntimeCellHandle,
    key: &str,
    value: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.string(value)?;
    values.array_set(array, key, value)
}

/// Writes one integer-keyed integer entry into a PHP array.
fn eval_array_set_int_int(
    array: RuntimeCellHandle,
    key: i64,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}
