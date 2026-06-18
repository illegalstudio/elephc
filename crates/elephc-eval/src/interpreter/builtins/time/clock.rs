//! Purpose:
//! Implements `time()` and `microtime()` eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` re-exports.
//!
//! Key details:
//! - Current timestamps are read from `SystemTime::now()` and converted into PHP
//!   integer or float runtime cells.

use super::super::super::*;

/// Evaluates PHP `time()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_time(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_time_result(values)
}

/// Returns the current Unix timestamp as a boxed PHP integer.
pub(in crate::interpreter) fn eval_time_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(eval_current_unix_timestamp()?)
}

/// Returns the current Unix timestamp as an integer payload.
pub(in crate::interpreter) fn eval_current_unix_timestamp() -> Result<i64, EvalStatus> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .as_secs();
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `microtime()` with an optional ignored argument.
pub(in crate::interpreter) fn eval_builtin_microtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_microtime_result(values),
        [as_float] => {
            let _ = eval_expr(as_float, context, scope, values)?;
            eval_microtime_result(values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the current Unix timestamp with microsecond precision as a boxed float.
pub(in crate::interpreter) fn eval_microtime_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let seconds = timestamp.as_secs() as f64;
    let micros = f64::from(timestamp.subsec_micros()) / 1_000_000.0;
    values.float(seconds + micros)
}
