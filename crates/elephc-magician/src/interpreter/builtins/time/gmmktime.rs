//! Purpose:
//! Eval registry entry and implementation wrapper for `gmmktime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - UTC timestamp construction delegates to the shared mktime helpers.

use super::*;

eval_builtin! {
    name: "gmmktime",
    area: Time,
    params: [hour, minute, second, month, day, year],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `gmmktime(hour, minute, second, month, day, year)`.
pub(in crate::interpreter) fn eval_builtin_gmmktime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_mktime_like("gmmktime", args, context, scope, values)
}

/// Converts PHP date components to a UTC Unix timestamp through libc `timegm`.
pub(in crate::interpreter) fn eval_gmmktime_result(
    hour: RuntimeCellHandle,
    minute: RuntimeCellHandle,
    second: RuntimeCellHandle,
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_mktime_result("gmmktime", hour, minute, second, month, day, year, context, values)
}
