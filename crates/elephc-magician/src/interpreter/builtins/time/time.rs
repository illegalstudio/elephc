//! Purpose:
//! Eval registry entry and implementation for `time`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - The current Unix timestamp helper is reused by date parsing and aliases.

use super::super::super::*;

eval_builtin! {
    name: "time",
    area: Time,
    params: [],
    direct: Time,
    values: Time,
}

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
