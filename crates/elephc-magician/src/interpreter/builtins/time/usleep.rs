//! Purpose:
//! Eval registry entry and implementation for `usleep`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Negative durations are rejected as runtime fatals and success returns PHP null.

use super::super::super::*;
use super::super::*;

eval_builtin! {
    name: "usleep",
    area: Time,
    params: [microseconds],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `usleep($microseconds)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_usleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [microseconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let microseconds = eval_expr(microseconds, context, scope, values)?;
    eval_usleep_result(microseconds, values)
}

/// Sleeps for a non-negative number of microseconds and returns PHP null.
pub(in crate::interpreter) fn eval_usleep_result(
    microseconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let microseconds = eval_int_value(microseconds, values)?;
    let microseconds = u64::try_from(microseconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_micros(microseconds));
    values.null()
}
