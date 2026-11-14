//! Purpose:
//! Dispatches contextual PHP diagnostics with reporting masks and balanced handler leases.
//!
//! Called from:
//! - Core trigger_error and interpreter engine warnings, including checked explicit casts.
//!
//! Key details:
//! - Callback arguments and suspended handler cells remain rooted until their owners transfer.
//! - Protected native failures enter eval catch state before temporary arguments are retired.

use super::super::super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// PHP engine warning severity.
pub(in crate::interpreter) const E_WARNING: i64 = 2;
/// PHP user-fatal severity.
pub(super) const E_USER_ERROR: i64 = 256;
/// PHP user-warning severity.
pub(super) const E_USER_WARNING: i64 = 512;
/// PHP user-notice severity.
pub(super) const E_USER_NOTICE: i64 = 1_024;
/// PHP engine deprecation severity.
pub(in crate::interpreter) const E_DEPRECATED: i64 = 8_192;
/// PHP user-deprecation severity.
pub(super) const E_USER_DEPRECATED: i64 = 16_384;

/// Dispatches one PHP diagnostic through the active handler and reporting mask.
pub(in crate::interpreter) fn eval_dispatch_php_error(
    message: &str,
    level: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let handled = dispatch_user_error_handler(message, level, context, values)?;
    let reporting_mask = match values.runtime_error_reporting(None) {
        Ok(mask) => mask,
        Err(EvalStatus::UnsupportedConstruct) => context.error_reporting_mask(),
        Err(status) => return Err(status),
    };
    if !handled && reporting_mask & level != 0 {
        values.warning_unhandled(&format_user_error(message, level, context))?;
    }
    Ok(handled)
}

/// Formats PHP's default user diagnostic with its category and source location.
fn format_user_error(message: &str, level: i64, context: &ElephcEvalContext) -> String {
    let category = match level {
        E_USER_ERROR => "Fatal error",
        E_WARNING | E_USER_WARNING => "Warning",
        E_DEPRECATED | E_USER_DEPRECATED => "Deprecated",
        _ => "Notice",
    };
    let (file, _, line, _) = context.call_site();
    format!("{category}: {message} in {file} on line {line}\n")
}

/// Invokes a handler with independent argument owners and converts its exact false sentinel.
fn dispatch_user_error_handler(message: &str, level: i64, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    let (file, _, line, _) = context.call_site();
    let arguments = (0..4).map(|_| RuntimeOwnerSlot::new(
        RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed())).collect::<Vec<_>>();
    with_runtime_owners(&arguments, || {
        let result = (|| {
            arguments[0].initialize(values.int(level)?);
            arguments[1].initialize(values.string(message)?);
            arguments[2].initialize(values.string(&file)?);
            arguments[3].initialize(values.int(line)?);
            let borrowed = arguments.iter().map(RuntimeOwnerSlot::borrowed).collect::<Vec<_>>();
            handler_result(level, &borrowed, context, values)
        })();
        let result = native_results::finish(result, context, values);
        let result = finish_synthesized_method_arguments(result,
            arguments.iter().map(RuntimeOwnerSlot::take), context, values)?;
        let inspected = (|| Ok(values.type_tag(result)? == EVAL_TAG_BOOL && !values.truthy(result)?))();
        let inspected = native_results::finish(inspected, context, values);
        let released = eval_release_value(context, values, result);
        inspected.and_then(|falls_through| released.map(|()| !falls_through))
    })
}

/// Borrows diagnostic operands while the native dispatcher or suspended Rust handler executes.
fn handler_result(level: i64, arguments: &[RuntimeCellHandle], context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.runtime_error_handler_dispatch(level, arguments) {
        Ok(Some(result)) => return Ok(result),
        Ok(None) => return values.bool_value(false),
        Err(EvalStatus::UnsupportedConstruct) => {},
        Err(status) => return Err(status),
    }
    let Some(handler) = context.error_handler_state().filter(|handler| handler.levels & level != 0) else {
        return values.bool_value(false);
    };
    let callback = eval_callable(handler.callback, context, values)?;
    let suspended = context.suspend_error_handler().expect("active handler was just checked");
    let result = with_runtime_owners(&suspended.callback, ||
        callback.with_values(context, values, |callback, context, values|
            eval_evaluated_callable_with_values(callback, arguments.to_vec(), context, values)));
    let discarded = context.resume_error_handler(suspended).map(|state| state.callback);
    finish_synthesized_method_arguments(result, discarded, context, values)
}
