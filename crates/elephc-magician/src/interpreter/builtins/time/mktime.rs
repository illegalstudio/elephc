//! Purpose:
//! Eval registry entry and implementation for `mktime` plus shared mktime helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - `gmmktime` and `strtotime` reuse the timestamp conversion helpers from this file.

use super::super::*;
use super::*;

eval_builtin! {
    contract: "mktime",
    area: Time,
    direct: Time,
    values: Time,
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
    if !(1..=6).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated = args
        .iter()
        .map(|arg| eval_expr(arg, context, scope, values))
        .collect::<Result<Vec<_>, _>>()?;
    eval_mktime_result_with_defaults(name, &evaluated, context, values)
}

/// Fills omitted or null optionals with the current local/UTC date part.
pub(in crate::interpreter) fn eval_mktime_result_with_defaults(
    name: &str,
    args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=6).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let date_name = if name == "gmmktime" { "gmdate" } else { "date" };
    let mut full = Vec::with_capacity(6);
    let mut temps = Vec::new();
    for (index, spec) in ["G", "i", "s", "n", "j", "Y"].into_iter().enumerate() {
        if let Some(arg) = args.get(index) {
            if !values.is_null(*arg)? {
                full.push(*arg);
                continue;
            }
        }
        match eval_current_date_part_int(date_name, spec, context, values) {
            Ok(default) => {
                temps.push(default);
                full.push(default);
            }
            Err(status) => {
                for temp in temps {
                    values.release(temp)?;
                }
                return Err(status);
            }
        }
    }
    let result = if name == "gmmktime" {
        eval_gmmktime_result(
            full[0], full[1], full[2], full[3], full[4], full[5], context, values,
        )
    } else {
        eval_mktime_result(
            name, full[0], full[1], full[2], full[3], full[4], full[5], context, values,
        )
    };
    for temp in temps {
        values.release(temp)?;
    }
    result
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
    let timestamp = unsafe { libc::mktime(&mut tm) };
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
    let timestamp = unsafe { libc::timegm(&mut tm) };
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
