//! Purpose:
//! Eval registry entry and implementation for `hrtime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Monotonic time is returned as nanoseconds or `[seconds, nanoseconds]`.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;


eval_builtin! {
    contract: "hrtime",
    area: Time,
    direct: Time,
    values: Time,
}

/// Evaluates PHP `hrtime($as_number = false)`.
pub(in crate::interpreter) fn eval_builtin_hrtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_hrtime_result(None, values),
        [as_number] => with_eval_operands(&[as_number], context, scope, values,
            |operands, _, _, values| eval_hrtime_result(Some(operands[0]), values)),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns monotonic time as either nanoseconds or `[seconds, nanoseconds]`.
pub(in crate::interpreter) fn eval_hrtime_result(
    as_number: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (seconds, nanoseconds) = eval_monotonic_time()?;
    let as_number = as_number
        .map(|value| values.truthy(value))
        .transpose()?
        .unwrap_or(false);
    if as_number {
        let total = seconds
            .checked_mul(1_000_000_000)
            .and_then(|value| value.checked_add(nanoseconds))
            .ok_or(EvalStatus::RuntimeFatal)?;
        return values.int(total);
    }
    let mut result = EvalArrayBuilder::indexed(values, 2)?;
    result.index(0, |values| values.int(seconds))?;
    result.index(1, |values| values.int(nanoseconds))?;
    Ok(result.finish())
}

/// Reads the monotonic clock in whole seconds and nanoseconds.
fn eval_monotonic_time() -> Result<(i64, i64), EvalStatus> {
    let mut timespec = MaybeUninit::<libc::timespec>::uninit();
    let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, timespec.as_mut_ptr()) };
    if status != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let timespec = unsafe { timespec.assume_init() };
    Ok((timespec.tv_sec, timespec.tv_nsec))
}
