//! Purpose:
//! Evaluates the `@` error-control operator and `throw` used as an expression.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr_with_result_ownership()`.
//!
//! Key details:
//! - `@` follows php-src's BEGIN/END_SILENCE: it keeps only the fatal levels of the
//!   reporting mask and restores the saved mask on every exit, including a throw, unless
//!   the operand itself installed a mask that reports non-fatal levels.
//! - The mask lives in the generated runtime when one is linked, so diagnostics raised by
//!   compiled code called from the operand are silenced too.
//! - A thrown value travels through the pending-Throwable channel that calls already use.

use super::*;

/// PHP's `E_FATAL_ERRORS`: the levels `@` keeps reporting.
const FATAL_ERROR_LEVELS: i64 = 1 | 4 | 16 | 64 | 256 | 4096;

/// Evaluates an `@` operand with non-fatal diagnostics hidden from the default output.
pub(super) fn eval_error_suppressed_expr(
    operand: &EvalExpr,
    own_result: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let saved = swap_error_reporting(None, context, values)?;
    let silenced = saved & !FATAL_ERROR_LEVELS != 0;
    if silenced {
        swap_error_reporting(Some(saved & FATAL_ERROR_LEVELS), context, values)?;
    }
    let result = eval_expr_with_result_ownership(operand, context, scope, values, own_result);
    if !silenced {
        return result;
    }
    let restored = restore_error_reporting(saved, context, values);
    match (result, restored) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(value), Err(status)) => {
            let _ = release_expr_result(value, context, values);
            Err(status)
        }
        (Err(status), _) => Err(status),
    }
}

/// Raises the operand of a `throw` expression through eval's Throwable channel.
pub(super) fn eval_throw_expr(
    operand: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let thrown = eval_owned_expr(operand, context, scope, values)?;
    let tag = values.type_tag(thrown);
    if tag != Ok(EVAL_TAG_OBJECT) {
        let _ = eval_release_value(context, values, thrown);
        tag?;
        return Err(EvalStatus::RuntimeFatal);
    }
    context.set_pending_throw(thrown);
    Err(EvalStatus::UncaughtThrowable)
}

/// Restores the mask saved by `@` unless the operand installed one with non-fatal levels.
fn restore_error_reporting(
    saved: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = swap_error_reporting(None, context, values)?;
    if current & !FATAL_ERROR_LEVELS == 0 {
        swap_error_reporting(Some(saved), context, values)?;
    }
    Ok(())
}

/// Reads, and optionally replaces, the active reporting mask and returns the previous one.
///
/// The generated runtime owns the mask when it is linked; a pure interpreter keeps it in
/// the eval context, as `error_reporting()` does.
fn swap_error_reporting(
    replacement: Option<i64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    match values.runtime_error_reporting(replacement) {
        Ok(previous) => Ok(previous),
        Err(EvalStatus::UnsupportedConstruct) => Ok(context.update_error_reporting(replacement)),
        Err(status) => Err(status),
    }
}
