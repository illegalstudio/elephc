//! Purpose:
//! Implements PHP `mktime()` conversion from local date components.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` re-exports.
//!
//! Key details:
//! - Component coercion checks libc integer bounds before calling `mktime`.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP `mktime(hour, minute, second, month, day, year)`.
pub(in crate::interpreter) fn eval_builtin_mktime(
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
    eval_mktime_result(hour, minute, second, month, day, year, values)
}

/// Converts PHP date components to a local Unix timestamp through libc `mktime`.
pub(in crate::interpreter) fn eval_mktime_result(
    hour: RuntimeCellHandle,
    minute: RuntimeCellHandle,
    second: RuntimeCellHandle,
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = eval_mktime_timestamp(
        eval_int_cell_as_c_int(hour, values)?,
        eval_int_cell_as_c_int(minute, values)?,
        eval_int_cell_as_c_int(second, values)?,
        eval_int_cell_as_c_int(month, values)?,
        eval_int_cell_as_c_int(day, values)?,
        eval_int_cell_as_c_int(year, values)?,
    )?;
    values.int(timestamp)
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
    let timestamp = unsafe { libc::mktime(&mut tm) };
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
