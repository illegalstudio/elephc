//! Purpose:
//! Implements the ext/posix eval operations backed by the shared PCNTL bridge: signal
//! delivery, terminal probing, identity queries, and the extension-local last error.
//!
//! Called from:
//! - The shared PCNTL evaluated-argument dispatcher.
//!
//! Key details:
//! - Errno state lives in the bridge, so eval and compiled code observe one `posix_get_last_error()`.
//! - `posix_isatty()` resolves eval-owned streams to their host descriptor; host stream payloads
//!   below the eval namespace already are native descriptors.

use super::*;
use crate::stream_resources::EVAL_RESOURCE_PAYLOAD_BASE;

/// Evaluates one ext/posix builtin, or returns `None` for names owned by another family.
pub(super) fn eval_posix_result(
    name: &str,
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "posix_errno" | "posix_get_last_error" => {
            eval_posix_no_args(args)?;
            values.int(elephc_pcntl::elephc_posix_get_last_error())?
        }
        "posix_geteuid" => {
            eval_posix_no_args(args)?;
            values.int(elephc_pcntl::elephc_posix_geteuid())?
        }
        "posix_getpgrp" => {
            eval_posix_no_args(args)?;
            values.int(elephc_pcntl::elephc_posix_getpgrp())?
        }
        "posix_isatty" => eval_posix_isatty(args, context, values)?,
        "posix_kill" => {
            let process_id = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
            let signal = eval_int_value(eval_pcntl_required_arg(args, 1)?.value, values)?;
            match elephc_pcntl::elephc_posix_kill(process_id, signal) {
                elephc_pcntl::POSIX_KILL_PID_OUT_OF_RANGE => eval_throw_builtin_value_error(
                    elephc_pcntl::POSIX_KILL_PID_RANGE_MESSAGE,
                    context,
                    values,
                )?,
                status => values.bool_value(status == 1)?,
            }
        }
        "posix_strerror" => eval_pcntl_strerror(args, values)?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Rejects any supplied argument for a zero-parameter ext/posix builtin.
fn eval_posix_no_args(args: &[Option<EvaluatedCallArg>]) -> Result<(), EvalStatus> {
    if args.iter().any(Option::is_some) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Probes an integer descriptor or stream resource and emits PHP's out-of-range warning.
fn eval_posix_isatty(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operand = eval_pcntl_required_arg(args, 0)?.value;
    let fd = if values.type_tag(operand)? == EVAL_TAG_RESOURCE {
        let payload = eval_resource_payload(operand, values)?;
        if payload >= EVAL_RESOURCE_PAYLOAD_BASE {
            match context.stream_resources().raw_fd(payload) {
                Some(fd) => i64::from(fd),
                None => return values.bool_value(false),
            }
        } else {
            payload
        }
    } else {
        eval_int_value(operand, values)?
    };
    match elephc_pcntl::elephc_posix_isatty(fd) {
        elephc_pcntl::POSIX_ISATTY_FD_OUT_OF_RANGE => {
            values.warning(elephc_pcntl::POSIX_ISATTY_FD_RANGE_WARNING)?;
            values.bool_value(false)
        }
        status => values.bool_value(status == 1),
    }
}
