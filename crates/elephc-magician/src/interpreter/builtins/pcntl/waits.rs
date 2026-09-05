//! Purpose:
//! Implements child-wait PCNTL adapters and by-reference output writeback.
//!
//! Called from:
//! - The shared PCNTL evaluated-argument dispatcher.
//!
//! Key details:
//! - Native status words stay target-specific and are decoded only by libc wrappers.
//! - Resource-usage and siginfo records use the same stable bridge layouts as AOT code.

use super::*;
use elephc_pcntl::{ElephcPcntlRUsage, ElephcPcntlSigInfo};

/// Evaluates `pcntl_wait`, `pcntl_waitpid`, or `pcntl_waitid`.
pub(super) fn eval_pcntl_wait_result(
    name: &str,
    args: &[Option<EvaluatedCallArg>],
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "pcntl_wait" => eval_pcntl_wait(args, mode, context, values)?,
        "pcntl_waitpid" => eval_pcntl_waitpid(args, mode, context, values)?,
        "pcntl_waitid" => eval_pcntl_waitid(args, mode, context, values)?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Waits for any child and writes status plus optional resource usage.
fn eval_pcntl_wait(
    args: &[Option<EvaluatedCallArg>],
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let status_arg = eval_pcntl_required_arg(args, 0)?;
    let flags = eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0, values)?;
    let usage_arg = eval_pcntl_arg(args, 2);
    let mut status = eval_int_value(status_arg.value, values)? as libc::c_int;
    let mut usage = ElephcPcntlRUsage::default();
    let pid = unsafe {
        if usage_arg.is_some() {
            elephc_pcntl::elephc_pcntl_wait4(
                -1,
                &mut status,
                flags as libc::c_int,
                &mut usage,
            )
        } else {
            elephc_pcntl::elephc_pcntl_wait(&mut status, flags as libc::c_int)
        }
    };
    let status = values.int(i64::from(status))?;
    eval_pcntl_write_ref(
        "pcntl_wait",
        1,
        "status",
        status_arg,
        status,
        mode,
        context,
        values,
    )?;
    if let Some(usage_arg) = usage_arg {
        let usage = if pid > 0 {
            eval_pcntl_rusage_array(&usage, values)?
        } else {
            values.assoc_new(0)?
        };
        eval_pcntl_write_ref(
            "pcntl_wait",
            3,
            "resource_usage",
            usage_arg,
            usage,
            mode,
            context,
            values,
        )?;
    }
    values.int(pid)
}

/// Waits for a selected child and writes status plus optional resource usage.
fn eval_pcntl_waitpid(
    args: &[Option<EvaluatedCallArg>],
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_id = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    let status_arg = eval_pcntl_required_arg(args, 1)?;
    let flags = eval_pcntl_optional_int(eval_pcntl_arg(args, 2), 0, values)?;
    let usage_arg = eval_pcntl_arg(args, 3);
    let mut status = eval_int_value(status_arg.value, values)? as libc::c_int;
    let mut usage = ElephcPcntlRUsage::default();
    let pid = unsafe {
        if usage_arg.is_some() {
            elephc_pcntl::elephc_pcntl_wait4(
                process_id,
                &mut status,
                flags as libc::c_int,
                &mut usage,
            )
        } else {
            elephc_pcntl::elephc_pcntl_waitpid(
                process_id,
                &mut status,
                flags as libc::c_int,
            )
        }
    };
    let status = values.int(i64::from(status))?;
    eval_pcntl_write_ref(
        "pcntl_waitpid",
        2,
        "status",
        status_arg,
        status,
        mode,
        context,
        values,
    )?;
    if let Some(usage_arg) = usage_arg {
        let usage = if pid > 0 {
            eval_pcntl_rusage_array(&usage, values)?
        } else {
            values.assoc_new(0)?
        };
        eval_pcntl_write_ref(
            "pcntl_waitpid",
            4,
            "resource_usage",
            usage_arg,
            usage,
            mode,
            context,
            values,
        )?;
    }
    values.int(pid)
}

/// Waits through `waitid` and conditionally writes its signal-information record.
fn eval_pcntl_waitid(
    args: &[Option<EvaluatedCallArg>],
    mode: PcntlCallMode,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id_type = eval_pcntl_optional_int(eval_pcntl_arg(args, 0), 0, values)?;
    let id = eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0, values)?;
    let info_arg = eval_pcntl_arg(args, 2);
    let flags = eval_pcntl_optional_int(eval_pcntl_arg(args, 3), 4, values)?;
    let usage_arg = eval_pcntl_arg(args, 4);
    let mut info = ElephcPcntlSigInfo::default();
    let mut usage = ElephcPcntlRUsage::default();
    let success = unsafe {
        elephc_pcntl::elephc_pcntl_waitid(
            id_type as libc::c_int,
            id,
            &mut info,
            flags as libc::c_int,
            if cfg!(target_os = "linux") && usage_arg.is_some() {
                &mut usage
            } else {
                std::ptr::null_mut()
            },
        )
    } != 0;
    if success {
        if let Some(info_arg) = info_arg {
            let info = eval_pcntl_siginfo_array(&info, values)?;
            eval_pcntl_write_ref(
                "pcntl_waitid",
                3,
                "info",
                info_arg,
                info,
                mode,
                context,
                values,
            )?;
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(usage_arg) = usage_arg {
        let usage = if success {
            eval_pcntl_rusage_array(&usage, values)?
        } else {
            values.assoc_new(0)?
        };
        eval_pcntl_write_ref(
            "pcntl_waitid",
            5,
            "resource_usage",
            usage_arg,
            usage,
            mode,
            context,
            values,
        )?;
    }
    values.bool_value(success)
}
