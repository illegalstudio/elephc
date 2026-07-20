//! Purpose:
//! Eval registry entry and implementation for `localtime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - `getdate` owns the shared timestamp coercion and array-entry helpers.

use super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "localtime",
    area: Time,
    params: [
        timestamp = EvalBuiltinDefaultValue::Null,
        associative = EvalBuiltinDefaultValue::Bool(false),
    ],
    direct: Time,
    values: Time,
}

const EVAL_LOCALTIME_KEYS: &[&str; 9] = &[
    "tm_sec", "tm_min", "tm_hour", "tm_mday", "tm_mon", "tm_year", "tm_wday", "tm_yday",
    "tm_isdst",
];

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
