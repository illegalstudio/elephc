//! Purpose:
//! Implements PHP `sleep()` and `usleep()` eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` re-exports.
//!
//! Key details:
//! - Negative durations are rejected as runtime fatals; successful sleeps return
//!   PHP-compatible values.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP `sleep($seconds)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_sleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [seconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let seconds = eval_expr(seconds, context, scope, values)?;
    eval_sleep_result(seconds, values)
}

/// Sleeps for a non-negative number of seconds and returns PHP's remaining-seconds value.
pub(in crate::interpreter) fn eval_sleep_result(
    seconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let seconds = eval_int_value(seconds, values)?;
    let seconds = u64::try_from(seconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    values.int(0)
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
