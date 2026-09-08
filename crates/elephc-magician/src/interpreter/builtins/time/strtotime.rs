//! Purpose:
//! Eval registry entry and implementation for `strtotime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Parsing delegates to the same vendored timelib implementation used by AOT code.

use super::super::*;
use super::*;

eval_builtin! {
    contract: "strtotime",
    area: Time,
    direct: Time,
    values: Time,
}

/// Evaluates PHP `strtotime(datetime, baseTimestamp = null)` for eval's supported subset.
pub(in crate::interpreter) fn eval_builtin_strtotime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [datetime] => {
            let datetime = eval_expr(datetime, context, scope, values)?;
            eval_strtotime_result(datetime, None, context, values)
        }
        [datetime, base_timestamp] => {
            let datetime = eval_expr(datetime, context, scope, values)?;
            let base_timestamp = eval_expr(base_timestamp, context, scope, values)?;
            eval_strtotime_result(datetime, Some(base_timestamp), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Parses one eval `strtotime()` input and boxes the resulting timestamp.
pub(in crate::interpreter) fn eval_strtotime_result(
    datetime: RuntimeCellHandle,
    base_timestamp: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(datetime)?;
    let base_timestamp = match base_timestamp {
        Some(base_timestamp) if !values.is_null(base_timestamp)? => {
            Some(eval_int_value(base_timestamp, values)?)
        }
        _ => None,
    };
    let input = String::from_utf8_lossy(&bytes);
    let timezone = eval_request_timezone(context, values)?;
    match elephc_tz::strtotime_timestamp_php(&input, base_timestamp, &timezone) {
        Some(timestamp) => values.int(timestamp),
        None => values.bool_value(false),
    }
}
