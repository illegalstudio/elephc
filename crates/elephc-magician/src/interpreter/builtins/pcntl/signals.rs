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
use crate::context::EvalPcntlSignalHandler;
use elephc_pcntl::ElephcPcntlSigInfo;

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
    if context.pcntl_async_signals() {
        eval_pcntl_dispatch_pending(context, values)?;
    }
    Ok(())
}

/// Queries or changes automatic signal dispatch and returns the prior state.
fn eval_pcntl_async_signals(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let previous = match eval_pcntl_arg(args, 0) {
        Some(enable) if !values.is_null(enable.value)? => {
            let enabled = values.truthy(enable.value)?;
            context.set_pcntl_async_signals(enabled)
        }
        Some(_) | None => context.pcntl_async_signals(),
    };
    values.bool_value(previous)
}

/// Installs an integer disposition or retained PHP callable for one signal.
fn eval_pcntl_signal(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signal = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    let handler = eval_pcntl_required_arg(args, 1)?.value;
    let restart = eval_pcntl_arg(args, 2)
        .map(|arg| values.truthy(arg.value))
        .transpose()?
        .unwrap_or(true);
    let (disposition, stored) = if matches!(values.type_tag(handler)?, EVAL_TAG_INT | EVAL_TAG_BOOL)
    {
        let disposition = eval_int_value(handler, values)?;
        let bridge_disposition = if (0..=1).contains(&disposition) {
            disposition as libc::c_int
        } else {
            3
        };
        (
            bridge_disposition,
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
    ) != 0;
    if !success {
        if let EvalPcntlSignalHandler::Callable(handler) = stored {
            values.release(handler)?;
        }
        return values.bool_value(false);
    }
    if let Some(previous) = context.set_pcntl_signal_handler(signal as libc::c_int, stored) {
        if let EvalPcntlSignalHandler::Callable(previous) = previous {
            values.release(previous)?;
        }
    }
    values.bool_value(true)
}

/// Returns the retained callable or integer disposition registered for a signal.
fn eval_pcntl_signal_get_handler(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signal = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    if signal < 1 || signal >= i64::from(elephc_pcntl::elephc_pcntl_signal_limit()) {
        return values.bool_value(false);
    }
    match context.pcntl_signal_handler(signal as libc::c_int) {
        Some(EvalPcntlSignalHandler::Callable(handler)) => values.retain(handler),
        Some(EvalPcntlSignalHandler::Disposition(disposition)) => values.int(disposition),
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
    let signals = eval_pcntl_int_array(eval_pcntl_required_arg(args, 1)?.value, values)?;
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
    let signals = eval_pcntl_int_array(eval_pcntl_required_arg(args, 0)?.value, values)?;
    let info_arg = eval_pcntl_arg(args, 1);
    let mut info = ElephcPcntlSigInfo::default();
    let signal = unsafe {
        if timed {
            let seconds = eval_pcntl_optional_int(eval_pcntl_arg(args, 2), 0, values)?;
            let nanoseconds = eval_pcntl_optional_int(eval_pcntl_arg(args, 3), 0, values)?;
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

/// Drains every complete signal record and invokes its registered PHP callback.
fn eval_pcntl_dispatch_pending(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    loop {
        let mut info = ElephcPcntlSigInfo::default();
        let status = unsafe { elephc_pcntl::elephc_pcntl_signal_next(&mut info) };
        match status {
            -1 => return Ok(false),
            0 => return Ok(true),
            _ => {}
        }
        let Some(EvalPcntlSignalHandler::Callable(handler)) =
            context.pcntl_signal_handler(info.signo as libc::c_int)
        else {
            continue;
        };
        let callback = values.retain(handler)?;
        let signal = values.int(info.signo)?;
        let info = eval_pcntl_siginfo_array(&info, values)?;
        let result = eval_call_user_func_with_values(
            vec![callback, signal, info],
            context,
            values,
        );
        values.release(callback)?;
        let result = result?;
        values.release(result)?;
    }
}
