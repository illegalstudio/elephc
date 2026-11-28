//! Purpose:
//! Eval registry entry and implementation for `getdate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - The timestamp conversion also serves other date and time builtins.

use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

use super::super::*;
use super::*;

eval_builtin! {
    contract: "getdate",
    area: Time,
    direct: Time,
    values: Time,
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
        [timestamp] => with_eval_operands(&[timestamp], context, scope, values,
            |operands, context, _, values| eval_getdate_result(Some(operands[0]), context, values)),
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
    let mut result = EvalArrayBuilder::assoc(values, 11)?;
    for (key, value) in [
        ("seconds", tm.tm_sec), ("minutes", tm.tm_min), ("hours", tm.tm_hour),
        ("mday", tm.tm_mday), ("wday", tm.tm_wday), ("mon", tm.tm_mon + 1),
        ("year", tm.tm_year + 1900), ("yday", tm.tm_yday),
    ] {
        result.string(key, |values| values.int(i64::from(value)))?;
    }
    for (key, value) in [
        ("weekday", EVAL_WEEKDAY_NAMES[eval_tm_weekday_index(&tm)?]),
        ("month", EVAL_MONTH_NAMES[eval_tm_month_index(&tm)?]),
    ] {
        result.string(key, |values| values.string(value))?;
    }
    result.index(0, |values| values.int(timestamp))?;
    Ok(result.finish())
}

/// Coerces an optional timestamp argument, treating null/omitted as the current time.
pub(in crate::interpreter) fn eval_optional_timestamp(
    timestamp: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    match timestamp {
        Some(timestamp) if !values.is_null(timestamp)? => eval_int_value(timestamp, values),
        _ => eval_current_unix_timestamp(),
    }
}
