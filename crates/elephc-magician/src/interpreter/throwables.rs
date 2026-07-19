//! Purpose:
//! Builds PHP Throwable objects for interpreter paths that need catchable runtime errors.
//!
//! Called from:
//! - `crate::interpreter::statements` and dynamic dispatch helpers.
//!
//! Key details:
//! - Helpers schedule the object in `ElephcEvalContext` and return `UncaughtThrowable`
//!   so surrounding try/catch execution can consume it.

use super::*;

/// Creates and schedules an `Error` through eval's normal Throwable channel.
pub(in crate::interpreter) fn eval_throw_error<T>(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("Error")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Creates and schedules a `TypeError` through eval's normal Throwable channel.
pub(in crate::interpreter) fn eval_throw_type_error<T>(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("TypeError")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
