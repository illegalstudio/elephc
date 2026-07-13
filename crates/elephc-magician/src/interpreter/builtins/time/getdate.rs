//! Purpose:
//! Eval registry entry and implementation for `getdate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - This file owns optional timestamp coercion and array-entry helpers reused by `localtime`.

use super::super::*;
use super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "getdate",
    area: Time,
    params: [timestamp = EvalBuiltinDefaultValue::Null],
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

/// Writes one string-keyed integer entry into a PHP array.
pub(in crate::interpreter) fn eval_array_set_string_int(
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
pub(in crate::interpreter) fn eval_array_set_string_str(
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
pub(in crate::interpreter) fn eval_array_set_int_int(
    array: RuntimeCellHandle,
    key: i64,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}
