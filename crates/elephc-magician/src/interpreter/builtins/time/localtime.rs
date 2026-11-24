//! Purpose:
//! Eval registry entry and implementation for `localtime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - `getdate` owns shared timestamp coercion.

use super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

eval_builtin! {
    contract: "localtime",
    area: Time,
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
        [timestamp] => with_eval_operands(&[timestamp], context, scope, values,
            |operands, context, _, values| eval_localtime_result(Some(operands[0]), None, context, values)),
        [timestamp, associative] => with_eval_operands(&[timestamp, associative], context, scope, values,
            |operands, context, _, values| eval_localtime_result(Some(operands[0]), Some(operands[1]), context, values)),
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
        let mut result = EvalArrayBuilder::assoc(values, fields.len())?;
        for (key, value) in EVAL_LOCALTIME_KEYS.iter().zip(fields) {
            result.string(key, |values| values.int(i64::from(value)))?;
        }
        return Ok(result.finish());
    }
    let mut result = EvalArrayBuilder::indexed(values, fields.len())?;
    for (index, value) in fields.into_iter().enumerate() {
        result.index(index, |values| values.int(i64::from(value)))?;
    }
    Ok(result.finish())
}
