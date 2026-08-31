//! Purpose:
//! Implements PCNTL signal registration, masks, synchronous waits, and safe dispatch.
//!
//! Called from:
//! - The shared PCNTL dispatcher and statement-level async safe points.
//!
//! Key details:
//! - OS handlers only enqueue stable records in the bridge self-pipe.
//! - PHP callbacks execute synchronously through Magician after delivery returns to safe code.

use super::*;
use crate::context::{pcntl_runtime, EvalPcntlSignalHandler};
use elephc_pcntl::{ElephcPcntlSigInfo, ElephcPcntlSignalMask};

/// Pins one handler-owner context across every fallible step of callback dispatch.
struct EvalHandlerDispatchGuard(*mut ElephcEvalContext);

impl Drop for EvalHandlerDispatchGuard {
    /// Releases the active-dispatch pin and finalizes a now-unreferenced detached context.
    fn drop(&mut self) {
        if pcntl_runtime::end_handler_dispatch(self.0) {
            unsafe {
                crate::ffi::context::drop_eval_context_now(self.0);
            }
        }
    }
}

/// Evaluates signal-related PCNTL functions.
pub(super) fn eval_pcntl_signal_result(
    name: &str,
    args: &[Option<EvaluatedCallArg>],
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "pcntl_async_signals" => eval_pcntl_async_signals(args, context, values)?,
        "pcntl_signal" => eval_pcntl_signal(args, context, values)?,
        "pcntl_signal_dispatch" => {
            if args.iter().any(Option::is_some) {
                return Err(EvalStatus::RuntimeFatal);
            }
            let success = eval_pcntl_dispatch_pending(context, values)?;
            values.bool_value(success)?
        }
        "pcntl_signal_get_handler" => eval_pcntl_signal_get_handler(args, context, values)?,
        "pcntl_sigprocmask" => {
            eval_pcntl_sigprocmask(args, mode, context, values)?
        }
        #[cfg(target_os = "linux")]
        "pcntl_sigwaitinfo" => {
            eval_pcntl_signal_wait(args, false, mode, context, values)?
        }
        #[cfg(target_os = "linux")]
        "pcntl_sigtimedwait" => {
            eval_pcntl_signal_wait(args, true, mode, context, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Dispatches queued signals before a statement when async mode is active.
pub(in crate::interpreter) fn eval_pcntl_maybe_dispatch(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if pcntl_runtime::async_signals() {
        eval_pcntl_dispatch_pending(context, values)?;
    }
    Ok(())
}

/// Queries or changes automatic signal dispatch and returns the prior state.
fn eval_pcntl_async_signals(
    args: &[Option<EvaluatedCallArg>],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let requested = match eval_pcntl_arg(args, 0) {
        Some(enable) if !values.is_null(enable.value)? => {
            let enabled = values.truthy(enable.value)?;
            Some(enabled)
        }
        Some(_) | None => None,
    };
    let previous = pcntl_runtime::update_async_signals(requested);
    values.bool_value(previous)
}

/// Installs an integer disposition or retained PHP callable for one signal.
fn eval_pcntl_signal(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signal = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    let signal_limit = i64::from(elephc_pcntl::elephc_pcntl_signal_limit());
    if signal < 1 {
        return eval_throw_builtin_value_error(
            "pcntl_signal(): Argument #1 ($signal) must be greater than or equal to 1",
            context,
            values,
        );
    }
    if signal >= signal_limit {
        return eval_throw_builtin_value_error(
            &format!(
                "pcntl_signal(): Argument #1 ($signal) must be less than {signal_limit}"
            ),
            context,
            values,
        );
    }
    let handler = eval_pcntl_required_arg(args, 1)?.value;
    let restart = eval_pcntl_arg(args, 2)
        .map(|arg| values.truthy(arg.value))
        .transpose()?
        .unwrap_or_else(|| default_restart_syscalls(signal));
    let handler_tag = values.type_tag(handler)?;
    if handler_tag == EVAL_TAG_BOOL {
        let literal = if values.truthy(handler)? { "true" } else { "false" };
        return eval_throw_type_error(
            &format!(
                "pcntl_signal(): Argument #2 ($handler) must be of type callable|int, {literal} given"
            ),
            context,
            values,
        );
    }
    let (disposition, stored) = if handler_tag == EVAL_TAG_INT {
        let disposition = eval_int_value(handler, values)?;
        if !(0..=1).contains(&disposition) {
            return eval_throw_builtin_value_error(
                "pcntl_signal(): Argument #2 ($handler) must be either SIG_DFL or SIG_IGN when an integer value is given",
                context,
                values,
            );
        }
        (
            disposition as libc::c_int,
            EvalPcntlSignalHandler::Disposition(disposition),
        )
    } else {
        eval_callable(handler, context, values)?;
        let retained = values.retain(handler)?;
        (2, EvalPcntlSignalHandler::Callable(retained))
    };
    let success = elephc_pcntl::elephc_pcntl_signal(
        signal as libc::c_int,
        disposition,
        libc::c_int::from(restart),
        elephc_pcntl::PCNTL_SIGNAL_OWNER_EVAL,
    ) != 0;
    if !success {
        if let EvalPcntlSignalHandler::Callable(handler) = stored {
            values.release(handler)?;
        }
        values.fatal(&format!(
            "Fatal error: Error installing signal handler for {signal}\n"
        ))?;
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(previous) = pcntl_runtime::replace_signal_handler(
        signal as libc::c_int,
        context as *mut ElephcEvalContext,
        stored,
    ) {
        release_replaced_pcntl_handler(previous, values)?;
    }
    values.bool_value(true)
}

/// Returns PHP's omitted-argument restart policy for one signal.
fn default_restart_syscalls(signal: i64) -> bool {
    signal != i64::from(libc::SIGALRM)
}

/// Returns the retained callable or integer disposition registered for a signal.
fn eval_pcntl_signal_get_handler(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signal = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    if signal < 1 || signal >= i64::from(elephc_pcntl::elephc_pcntl_signal_limit()) {
        return eval_throw_builtin_value_error(
            &format!(
                "pcntl_signal_get_handler(): Argument #1 ($signal) must be between 1 and {}",
                elephc_pcntl::elephc_pcntl_signal_limit() - 1
            ),
            context,
            values,
        );
    }
    if elephc_pcntl::elephc_pcntl_signal_owner(signal as libc::c_int)
        == elephc_pcntl::PCNTL_SIGNAL_OWNER_AOT
    {
        return match values.pcntl_aot_signal_handler(signal)? {
            Some(handler) => Ok(handler),
            None => values.int(0),
        };
    }
    match pcntl_runtime::signal_handler(signal as libc::c_int) {
        Some(entry @ pcntl_runtime::EvalPcntlSignalEntry {
            handler: EvalPcntlSignalHandler::Callable(handler),
            ..
        }) => {
            let retained = values.retain(handler)?;
            if !std::ptr::eq(entry.context, context as *mut ElephcEvalContext) {
                if let Some(owner) = pcntl_runtime::begin_callable_use(handler, context) {
                    context.retain_pcntl_foreign_callable(handler, owner);
                }
            }
            Ok(retained)
        }
        Some(pcntl_runtime::EvalPcntlSignalEntry {
            handler: EvalPcntlSignalHandler::Disposition(disposition),
            ..
        }) => values.int(disposition),
        None => values.int(0),
    }
}

/// Changes the native signal mask and optionally writes its previous members.
fn eval_pcntl_sigprocmask(
    args: &[Option<EvaluatedCallArg>],
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let how = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    let signals = eval_pcntl_int_array(
        eval_pcntl_required_arg(args, 1)?.value,
        "pcntl_sigprocmask",
        2,
        "signals",
        "signals",
        context,
        values,
    )?;
    if !matches!(
        how as libc::c_int,
        libc::SIG_BLOCK | libc::SIG_UNBLOCK | libc::SIG_SETMASK
    ) {
        return eval_throw_builtin_value_error(
            "pcntl_sigprocmask(): Argument #1 ($mode) must be one of SIG_BLOCK, SIG_UNBLOCK, or SIG_SETMASK",
            context,
            values,
        );
    }
    eval_validate_pcntl_signal_set(
        "pcntl_sigprocmask",
        2,
        &signals,
        how as libc::c_int == libc::SIG_SETMASK,
        context,
        values,
    )?;
    let old_arg = eval_pcntl_arg(args, 2);
    let mut old = vec![0i64; elephc_pcntl::elephc_pcntl_signal_limit() as usize];
    let count = unsafe {
        elephc_pcntl::elephc_pcntl_sigprocmask(
            how as libc::c_int,
            signals.as_ptr(),
            signals.len(),
            if old_arg.is_some() {
                old.as_mut_ptr()
            } else {
                std::ptr::null_mut()
            },
            old.len(),
        )
    };
    if count < 0 {
        return values.bool_value(false);
    }
    if let Some(old_arg) = old_arg {
        old.truncate(count as usize);
        let old = eval_pcntl_indexed_int_array(&old, values)?;
        eval_pcntl_write_ref(
            "pcntl_sigprocmask",
            3,
            "old_signals",
            old_arg,
            old,
            mode,
            context,
            values,
        )?;
    }
    values.bool_value(true)
}

/// Waits synchronously for a selected Linux signal and conditionally writes siginfo.
#[cfg(target_os = "linux")]
fn eval_pcntl_signal_wait(
    args: &[Option<EvaluatedCallArg>],
    timed: bool,
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = if timed {
        "pcntl_sigtimedwait"
    } else {
        "pcntl_sigwaitinfo"
    };
    let signals = eval_pcntl_int_array(
        eval_pcntl_required_arg(args, 0)?.value,
        name,
        1,
        "signals",
        "signals",
        context,
        values,
    )?;
    eval_validate_pcntl_signal_set(name, 1, &signals, false, context, values)?;
    let info_arg = eval_pcntl_arg(args, 1);
    let mut info = ElephcPcntlSigInfo::default();
    let signal = unsafe {
        if timed {
            let seconds = eval_pcntl_optional_int(eval_pcntl_arg(args, 2), 0, values)?;
            let nanoseconds = eval_pcntl_optional_int(eval_pcntl_arg(args, 3), 0, values)?;
            if seconds < 0 {
                return eval_throw_builtin_value_error(
                    "pcntl_sigtimedwait(): Argument #3 ($seconds) must be greater than or equal to 0",
                    context,
                    values,
                );
            }
            if !(0..1_000_000_000).contains(&nanoseconds) {
                return eval_throw_builtin_value_error(
                    "pcntl_sigtimedwait(): Argument #4 ($nanoseconds) must be between 0 and 1e9",
                    context,
                    values,
                );
            }
            if seconds == 0 && nanoseconds == 0 {
                return eval_throw_builtin_value_error(
                    "pcntl_sigtimedwait(): At least one of argument #3 ($seconds) or argument #4 ($nanoseconds) must be greater than 0",
                    context,
                    values,
                );
            }
            elephc_pcntl::elephc_pcntl_sigtimedwait(
                signals.as_ptr(),
                signals.len(),
                &mut info,
                seconds,
                nanoseconds,
            )
        } else {
            elephc_pcntl::elephc_pcntl_sigwaitinfo(
                signals.as_ptr(),
                signals.len(),
                &mut info,
            )
        }
    };
    if signal < 0 {
        return values.bool_value(false);
    }
    if let Some(info_arg) = info_arg {
        let info = eval_pcntl_siginfo_array(&info, values)?;
        let (function, number) = if timed {
            ("pcntl_sigtimedwait", 2)
        } else {
            ("pcntl_sigwaitinfo", 2)
        };
        eval_pcntl_write_ref(
            function,
            number,
            "info",
            info_arg,
            info,
            mode,
            context,
            values,
        )?;
    }
    values.int(signal)
}

/// Raises PHP's signal-array `ValueError`s before an eval bridge call reaches libc.
fn eval_validate_pcntl_signal_set(
    name: &str,
    argument: usize,
    signals: &[i64],
    allow_empty: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if signals.is_empty() && !allow_empty {
        return eval_throw_builtin_value_error(
            &format!("{name}(): Argument #{argument} ($signals) must not be empty"),
            context,
            values,
        );
    }
    let maximum = i64::from(elephc_pcntl::elephc_pcntl_signal_limit()) - 1;
    if signals
        .iter()
        .any(|signal| !(1..=maximum).contains(signal))
    {
        return eval_throw_builtin_value_error(
            &format!(
                "{name}(): Argument #{argument} ($signals) signals must be between 1 and {maximum}"
            ),
            context,
            values,
        );
    }
    Ok(())
}

/// Drains one masked signal snapshot and invokes its registered PHP callbacks.
fn eval_pcntl_dispatch_pending(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !pcntl_runtime::begin_dispatch() {
        return Ok(true);
    }
    let mut previous_mask = ElephcPcntlSignalMask::default();
    if unsafe { elephc_pcntl::elephc_pcntl_dispatch_begin(&mut previous_mask) } == 0 {
        pcntl_runtime::end_dispatch();
        return Ok(false);
    }
    let published = values.set_pcntl_dispatching(true);
    let result = match published {
        Ok(()) => eval_pcntl_dispatch_masked_snapshot(context, values),
        Err(status) => Err(status),
    };
    if result.is_err() {
        eval_pcntl_discard_masked_snapshot();
    }
    let restored = unsafe { elephc_pcntl::elephc_pcntl_dispatch_end(&previous_mask) } != 0;
    pcntl_runtime::end_dispatch();
    let unpublished = values.set_pcntl_dispatching(false);
    match result {
        Ok(success) => {
            unpublished?;
            Ok(success && restored)
        }
        Err(status) => {
            let _ = unpublished;
            Err(status)
        }
    }
}

/// Invokes every callable record already present after signal delivery has been masked.
fn eval_pcntl_dispatch_masked_snapshot(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    loop {
        let mut info = ElephcPcntlSigInfo::default();
        let status = unsafe {
            elephc_pcntl::elephc_pcntl_signal_next(
                &mut info,
                elephc_pcntl::PCNTL_SIGNAL_OWNER_EVAL,
            )
        };
        match status {
            -1 => return Ok(false),
            0 => return Ok(true),
            _ => {}
        }
        let Some(entry) = pcntl_runtime::begin_handler_dispatch(info.signo as libc::c_int)
        else {
            continue;
        };
        let _dispatch_guard = EvalHandlerDispatchGuard(entry.context);
        let EvalPcntlSignalHandler::Callable(handler) = entry.handler else {
            continue;
        };
        let callback = values.retain(handler)?;
        let signal = values.int(info.signo)?;
        let info = eval_pcntl_siginfo_array(&info, values)?;
        let current_context = context as *mut ElephcEvalContext;
        let owner_context = if entry.context.is_null() {
            current_context
        } else {
            entry.context
        };
        let result = unsafe {
            eval_call_user_func_with_values(
                vec![callback, signal, info],
                &mut *owner_context,
                values,
            )
        };
        let result = match result {
            Ok(result) => values.release(result),
            Err(status) => {
                if owner_context != current_context {
                    if let Some(thrown) = unsafe { &mut *owner_context }.take_pending_throw() {
                        context.set_pending_throw(thrown);
                    }
                }
                Err(status)
            }
        };
        let callback_release = values.release(callback);
        callback_release?;
        result?;
    }
}

/// Releases a replaced callable and reclaims its detached owner after the last handler leaves it.
fn release_replaced_pcntl_handler(
    previous: pcntl_runtime::EvalPcntlSignalEntry,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let EvalPcntlSignalHandler::Callable(callback) = previous.handler {
        values.release(callback)?;
    }
    if pcntl_runtime::take_collectable_context(previous.context) {
        unsafe {
            crate::ffi::context::drop_eval_context_now(previous.context);
        }
    }
    Ok(())
}

/// Discards the rest of a masked snapshot after one handler propagates a Throwable.
fn eval_pcntl_discard_masked_snapshot() {
    let mut info = ElephcPcntlSigInfo::default();
    while unsafe {
        elephc_pcntl::elephc_pcntl_signal_next(
            &mut info,
            elephc_pcntl::PCNTL_SIGNAL_OWNER_EVAL,
        )
    } == 1
    {}
}

#[cfg(test)]
mod tests {
    use super::default_restart_syscalls;

    /// Keeps SIGALRM interruptible while retaining PHP's restart default for other signals.
    #[test]
    fn omitted_restart_syscalls_is_false_only_for_sigalrm() {
        assert!(!default_restart_syscalls(i64::from(libc::SIGALRM)));
        assert!(default_restart_syscalls(i64::from(libc::SIGUSR1)));
    }
}
