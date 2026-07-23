//! Purpose:
//! Eval registry entry and implementation for `mktime` plus shared mktime helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - `gmmktime` and `strtotime` reuse the timestamp conversion helpers from this file.
//! - Windows uses the CRT's explicit 64-bit timestamp entry points.

use super::super::*;
use super::*;

eval_builtin! {
    name: "mktime",
    area: Time,
    params: [hour, minute, second, month, day, year],
    direct: Time,
    values: Time,
}

#[cfg(windows)]
unsafe extern "C" {
    /// Converts local broken-down time into a 64-bit Unix timestamp through the Windows CRT.
    #[link_name = "_mktime64"]
    fn windows_mktime64(time: *mut libc::tm) -> i64;

    /// Converts UTC broken-down time into a 64-bit Unix timestamp through the Windows CRT.
    #[link_name = "_mkgmtime64"]
    fn windows_mkgmtime64(time: *mut libc::tm) -> i64;
}

/// Evaluates PHP `mktime(hour, minute, second, month, day, year)`.
pub(in crate::interpreter) fn eval_builtin_mktime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_mktime_like("mktime", args, context, scope, values)
}

/// Evaluates PHP `mktime(hour, minute, second, month, day, year)`.
pub(in crate::interpreter) fn eval_builtin_mktime_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hour, minute, second, month, day, year] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hour = eval_expr(hour, context, scope, values)?;
    let minute = eval_expr(minute, context, scope, values)?;
    let second = eval_expr(second, context, scope, values)?;
    let month = eval_expr(month, context, scope, values)?;
    let day = eval_expr(day, context, scope, values)?;
    let year = eval_expr(year, context, scope, values)?;
    eval_mktime_result(name, hour, minute, second, month, day, year, context, values)
}

/// Converts PHP date components to a local Unix timestamp through libc `mktime`.
pub(in crate::interpreter) fn eval_mktime_result(
    name: &str,
    hour: RuntimeCellHandle,
    minute: RuntimeCellHandle,
    second: RuntimeCellHandle,
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = (
        eval_int_cell_as_c_int(hour, values)?,
        eval_int_cell_as_c_int(minute, values)?,
        eval_int_cell_as_c_int(second, values)?,
        eval_int_cell_as_c_int(month, values)?,
        eval_int_cell_as_c_int(day, values)?,
        eval_int_cell_as_c_int(year, values)?,
    );
    let timestamp = match name {
        "mktime" => eval_context_mktime_timestamp(args, context)?,
        "gmmktime" => eval_gmmktime_timestamp(args)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.int(timestamp)
}

/// Converts local date components into an eval-timezone Unix timestamp.
pub(in crate::interpreter) fn eval_context_mktime_timestamp(
    args: (
        libc::c_int,
        libc::c_int,
        libc::c_int,
        libc::c_int,
        libc::c_int,
        libc::c_int,
    ),
    context: &ElephcEvalContext,
) -> Result<i64, EvalStatus> {
    eval_with_timezone(context.default_timezone(), || {
        eval_mktime_timestamp(args.0, args.1, args.2, args.3, args.4, args.5)
    })
}

/// Converts local date components into a Unix timestamp through libc `mktime`.
pub(in crate::interpreter) fn eval_mktime_timestamp(
    hour: libc::c_int,
    minute: libc::c_int,
    second: libc::c_int,
    month: libc::c_int,
    day: libc::c_int,
    year: libc::c_int,
) -> Result<i64, EvalStatus> {
    let mut tm = unsafe { MaybeUninit::<libc::tm>::zeroed().assume_init() };
    tm.tm_hour = hour;
    tm.tm_min = minute;
    tm.tm_sec = second;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_year = year - 1900;
    tm.tm_isdst = -1;
    #[cfg(unix)]
    let timestamp = unsafe { libc::mktime(&mut tm) };
    #[cfg(windows)]
    let timestamp = unsafe { windows_mktime64(&mut tm) };
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Converts UTC date components into a Unix timestamp through libc `timegm`.
pub(in crate::interpreter) fn eval_gmmktime_timestamp(
    args: (
        libc::c_int,
        libc::c_int,
        libc::c_int,
        libc::c_int,
        libc::c_int,
        libc::c_int,
    ),
) -> Result<i64, EvalStatus> {
    let mut tm = unsafe { MaybeUninit::<libc::tm>::zeroed().assume_init() };
    tm.tm_hour = args.0;
    tm.tm_min = args.1;
    tm.tm_sec = args.2;
    tm.tm_mon = args.3 - 1;
    tm.tm_mday = args.4;
    tm.tm_year = args.5 - 1900;
    tm.tm_isdst = 0;
    #[cfg(unix)]
    let timestamp = unsafe { libc::timegm(&mut tm) };
    #[cfg(windows)]
    let timestamp = unsafe { windows_mkgmtime64(&mut tm) };
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Casts one eval cell to a PHP int and checks it fits a libc `c_int`.
pub(in crate::interpreter) fn eval_int_cell_as_c_int(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<libc::c_int, EvalStatus> {
    let value = eval_int_value(value, values)?;
    libc::c_int::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}
