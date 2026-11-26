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
//!   Throwing a non-object or an object that is not `Throwable` raises PHP's catchable `Error`
//!   instead, for the `throw` statement and expression alike.
//! - A property fetch in a `const` initializer goes through a receiver check: PHP allows only
//!   an enum case there, or `null` behind `?->`.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

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
    let result = result.map(RuntimeOwnerSlot::new);
    with_runtime_owners(&result.as_ref().ok(), || {
        let restored = restore_error_reporting(saved, context, values);
        match (&result, restored) {
            (Ok(value), Ok(())) => Ok(value.take()),
            (Ok(value), Err(status)) => {
                let _ = release_expr_result(value.take(), context, values);
                Err(status)
            }
            (Err(status), _) => Err(*status),
        }
    })
}

/// Raises the operand of a `throw` expression through eval's Throwable channel.
pub(super) fn eval_throw_expr(
    operand: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let thrown = eval_owned_expr(operand, context, scope, values)?;
    let thrown = eval_throwable_operand(thrown, context, values)?;
    context.set_pending_throw(thrown);
    Err(EvalStatus::UncaughtThrowable)
}

/// Accepts an owned `throw` operand that implements `Throwable`.
///
/// Any other value is released and replaced by PHP's `Error`, raised through the pending
/// Throwable channel so an enclosing `catch` sees it.
pub(in crate::interpreter) fn eval_throwable_operand(
    thrown: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let thrown = RuntimeOwnerSlot::new(thrown);
    with_runtime_owners(&thrown, || {
        let throwable = match values.type_tag(thrown.borrowed()) {
            Ok(EVAL_TAG_OBJECT) => eval_instanceof_object_result(
                thrown.borrowed(),
                "Throwable",
                context,
                values,
            )
            .map(Some),
            Ok(_) => Ok(None),
            Err(status) => Err(status),
        };
        match throwable {
            Ok(Some(true)) => Ok(thrown.take()),
            Ok(found) => {
                eval_release_value(context, values, thrown.take())?;
                let message = if found.is_some() {
                    "Cannot throw objects that do not implement Throwable"
                } else {
                    "Can only throw objects"
                };
                throwables::eval_throw_error(message, context, values)
            }
            Err(status) => {
                let _ = eval_release_value(context, values, thrown.take());
                Err(status)
            }
        }
    })
}

/// Evaluates the receiver of a property fetch in a `const` initializer.
///
/// PHP evaluates such a fetch only on an enum case; `nullsafe` also lets `null` through, which
/// the `?->` fetch then turns into `null`. Any other receiver raises PHP's `Error`.
pub(super) fn eval_constant_property_receiver(
    object: &EvalExpr,
    nullsafe: bool,
    own_result: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = RuntimeOwnerSlot::new(eval_expr_with_result_ownership(
        object,
        context,
        scope,
        values,
        own_result,
    )?);
    with_runtime_owners(&receiver, || {
        let allowed = match values.type_tag(receiver.borrowed()) {
            Ok(EVAL_TAG_NULL) => Ok(nullsafe),
            Ok(EVAL_TAG_OBJECT) => eval_instanceof_object_result(
                receiver.borrowed(),
                "UnitEnum",
                context,
                values,
            ),
            Ok(_) => Ok(false),
            Err(status) => Err(status),
        };
        match allowed {
            Ok(true) => Ok(receiver.take()),
            Ok(false) => {
                release_expr_result(receiver.take(), context, values)?;
                throwables::eval_throw_error(
                    "Fetching properties on non-enums in constant expressions is not allowed",
                    context,
                    values,
                )
            }
            Err(status) => {
                let _ = release_expr_result(receiver.take(), context, values);
                Err(status)
            }
        }
    })
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
