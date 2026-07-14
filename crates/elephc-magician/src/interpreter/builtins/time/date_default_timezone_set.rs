//! Purpose:
//! Eval registry entry and implementation for `date_default_timezone_set`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - The timezone identifier is stored on the eval context.

use super::super::super::*;

eval_builtin! {
    name: "date_default_timezone_set",
    area: Time,
    params: [timezoneId],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `date_default_timezone_set($timezoneId)`.
pub(in crate::interpreter) fn eval_builtin_date_default_timezone_set(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [timezone] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let timezone = eval_expr(timezone, context, scope, values)?;
    eval_date_default_timezone_set_result(timezone, context, values)
}

/// Stores one eval-local default timezone identifier and reports success.
pub(in crate::interpreter) fn eval_date_default_timezone_set_result(
    timezone: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timezone = values.string_bytes(timezone)?;
    let timezone = String::from_utf8_lossy(&timezone).into_owned();
    context.set_default_timezone(timezone);
    values.bool_value(true)
}
