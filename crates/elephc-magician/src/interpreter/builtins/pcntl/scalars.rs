//! Purpose:
//! Implements scalar and CPU-affinity PCNTL eval operations.
//!
//! Called from:
//! - The shared PCNTL evaluated-argument dispatcher.
//!
//! Key details:
//! - Optional null process identifiers become zero, matching AOT lowering.
//! - Priority retrieval keeps `-1` distinct from bridge failure.

use super::*;

#[cfg(target_os = "linux")]
const PCNTL_CPU_CAPACITY: usize = 1024;

/// Evaluates scalar PCNTL functions backed directly by the shared bridge.
pub(super) fn eval_pcntl_scalar_result(
    name: &str,
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "pcntl_alarm" => {
            let seconds = eval_pcntl_required_arg(args, 0)?;
            let seconds = eval_int_value(seconds.value, values)?;
            values.int(elephc_pcntl::elephc_pcntl_alarm(seconds))?
        }
        "pcntl_fork" => {
            if args.iter().any(Option::is_some) {
                return Err(EvalStatus::RuntimeFatal);
            }
            let process_id = elephc_pcntl::elephc_pcntl_fork();
            if process_id == -1 {
                values.warning(&elephc_pcntl::pcntl_last_error_warning(
                    elephc_pcntl::PCNTL_WARNING_FORK,
                ))?;
            }
            values.int(process_id)?
        }
        "pcntl_errno" | "pcntl_get_last_error" => {
            if args.iter().any(Option::is_some) {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.int(i64::from(elephc_pcntl::elephc_pcntl_get_last_error()))?
        }
        "pcntl_getpriority" => eval_pcntl_getpriority(args, values)?,
        "pcntl_setpriority" => eval_pcntl_setpriority(args, values)?,
        "pcntl_strerror" => eval_pcntl_strerror(args, values)?,
        "pcntl_wifcontinued" | "pcntl_wifexited" | "pcntl_wifsignaled"
        | "pcntl_wifstopped" | "pcntl_wexitstatus" | "pcntl_wstopsig"
        | "pcntl_wtermsig" => eval_pcntl_wait_status_result(name, args, values)?,
        #[cfg(target_os = "linux")]
        "pcntl_getcpu" => {
            if args.iter().any(Option::is_some) {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.int(elephc_pcntl::elephc_pcntl_getcpu())?
        }
        #[cfg(target_os = "linux")]
        "pcntl_getcpuaffinity" => eval_pcntl_getcpuaffinity(args, values)?,
        #[cfg(target_os = "linux")]
        "pcntl_setcpuaffinity" => eval_pcntl_setcpuaffinity(args, values)?,
        #[cfg(target_os = "linux")]
        "pcntl_setns" => eval_pcntl_setns(args, values)?,
        #[cfg(target_os = "linux")]
        "pcntl_unshare" => eval_pcntl_unshare(args, values)?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Converts an omitted or null eval argument to a fixed integer default.
pub(super) fn eval_pcntl_optional_int(
    arg: Option<&EvaluatedCallArg>,
    default: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    match arg {
        None => Ok(default),
        Some(arg) if values.is_null(arg.value)? => Ok(default),
        Some(arg) => eval_int_value(arg.value, values),
    }
}

/// Returns a priority or PHP false without conflating a valid `-1` priority with failure.
fn eval_pcntl_getpriority(
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 0), 0, values)?;
    let mode = eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0, values)?;
    let mut priority = 0;
    let success = unsafe {
        elephc_pcntl::elephc_pcntl_getpriority(process_id, mode as libc::c_int, &mut priority)
    };
    if success == 0 {
        values.bool_value(false)
    } else {
        values.int(i64::from(priority))
    }
}

/// Changes a process priority and returns the bridge success flag.
fn eval_pcntl_setpriority(
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let priority = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0, values)?;
    let mode = eval_pcntl_optional_int(eval_pcntl_arg(args, 2), 0, values)?;
    values.bool_value(
        elephc_pcntl::elephc_pcntl_setpriority(
            priority as libc::c_int,
            process_id,
            mode as libc::c_int,
        ) != 0,
    )
}

/// Copies libc's borrowed `strerror` bytes into a PHP string cell.
fn eval_pcntl_strerror(
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let error = eval_pcntl_required_arg(args, 0)?;
    let error = eval_int_value(error.value, values)?;
    let mut len = 0usize;
    let pointer = unsafe {
        elephc_pcntl::elephc_pcntl_strerror(error as libc::c_int, &mut len)
    };
    if pointer.is_null() {
        return values.string("");
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer, len) };
    values.string_bytes_value(bytes)
}

/// Decodes one target-native child status through the bridge's libc wrappers.
fn eval_pcntl_wait_status_result(
    name: &str,
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let status = eval_pcntl_required_arg(args, 0)?;
    let status = eval_int_value(status.value, values)? as libc::c_int;
    match name {
        "pcntl_wifcontinued" => {
            values.bool_value(elephc_pcntl::elephc_pcntl_wifcontinued(status) != 0)
        }
        "pcntl_wifexited" => {
            values.bool_value(elephc_pcntl::elephc_pcntl_wifexited(status) != 0)
        }
        "pcntl_wifsignaled" => {
            values.bool_value(elephc_pcntl::elephc_pcntl_wifsignaled(status) != 0)
        }
        "pcntl_wifstopped" => {
            values.bool_value(elephc_pcntl::elephc_pcntl_wifstopped(status) != 0)
        }
        "pcntl_wexitstatus" => {
            values.int(i64::from(elephc_pcntl::elephc_pcntl_wexitstatus(status)))
        }
        "pcntl_wstopsig" => {
            values.int(i64::from(elephc_pcntl::elephc_pcntl_wstopsig(status)))
        }
        "pcntl_wtermsig" => {
            values.int(i64::from(elephc_pcntl::elephc_pcntl_wtermsig(status)))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the selected Linux process CPU mask as a PHP array or false.
#[cfg(target_os = "linux")]
fn eval_pcntl_getcpuaffinity(
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 0), 0, values)?;
    let mut cpus = vec![0i64; PCNTL_CPU_CAPACITY];
    let count = unsafe {
        elephc_pcntl::elephc_pcntl_getcpuaffinity(
            process_id,
            cpus.as_mut_ptr(),
            cpus.len(),
        )
    };
    if count < 0 {
        return values.bool_value(false);
    }
    cpus.truncate(count as usize);
    eval_pcntl_indexed_int_array(&cpus, values)
}

/// Replaces the selected Linux process CPU mask.
#[cfg(target_os = "linux")]
fn eval_pcntl_setcpuaffinity(
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 0), 0, values)?;
    let cpus = eval_pcntl_int_array(eval_pcntl_required_arg(args, 1)?.value, values)?;
    let success = unsafe {
        elephc_pcntl::elephc_pcntl_setcpuaffinity(process_id, cpus.as_ptr(), cpus.len())
    };
    values.bool_value(success != 0)
}

/// Joins the selected Linux process namespace.
#[cfg(target_os = "linux")]
fn eval_pcntl_setns(
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 0), 0, values)?;
    let namespace_type =
        eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0x4000_0000, values)?;
    values.bool_value(
        elephc_pcntl::elephc_pcntl_setns(process_id, namespace_type as libc::c_int) != 0,
    )
}

/// Disassociates the requested Linux execution contexts.
#[cfg(target_os = "linux")]
fn eval_pcntl_unshare(
    args: &[Option<EvaluatedCallArg>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = eval_pcntl_required_arg(args, 0)?;
    let flags = eval_int_value(flags.value, values)?;
    values.bool_value(elephc_pcntl::elephc_pcntl_unshare(flags as libc::c_int) != 0)
}
