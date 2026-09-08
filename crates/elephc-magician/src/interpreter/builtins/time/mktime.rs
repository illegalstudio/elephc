//! Purpose:
//! Eval registry entry and implementation for `mktime` plus shared mktime helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Both local and UTC variants delegate to the vendored timelib bridge.

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
    eval_mktime_with_clock(name, args, context, values, eval_current_unix_timestamp)
}

/// Reads a single clock sample after argument evaluation; the injected clock is a test seam.
pub(in crate::interpreter) fn eval_mktime_with_clock(
    name: &str,
    args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    clock: impl FnOnce() -> Result<i64, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=6).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let timezone = match name {
        "mktime" => eval_request_timezone(context, values)?,
        "gmmktime" => "UTC".to_owned(),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let current = eval_timezone_broken_down_time(clock()?, &timezone)?;
    let defaults = [current.tm_hour, current.tm_min, current.tm_sec,
        current.tm_mon + 1, current.tm_mday, current.tm_year + 1900];
    let mut full = Vec::with_capacity(6);
    let mut temps = Vec::new();
    for (index, default) in defaults.into_iter().enumerate() {
        if let Some(arg) = args.get(index) {
            if index == 0 || !values.is_null(*arg)? {
                full.push(*arg);
                continue;
            }
        }
        match values.int(default) {
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

/// Converts PHP date components to a local Unix timestamp through frozen timelib.
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
    let args = [hour, minute, second, month, day, year]
        .map(|value| eval_int_value(value, values))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let timezone = match name {
        "mktime" => eval_request_timezone(context, values)?,
        "gmmktime" => "UTC".to_owned(),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    match elephc_tz::mktime_timestamp_php(
        args[0], args[1], args[2], args[3], args[4], args[5], &timezone,
    ) {
        Some(timestamp) => values.int(timestamp),
        None => values.bool_value(false),
    }
}
