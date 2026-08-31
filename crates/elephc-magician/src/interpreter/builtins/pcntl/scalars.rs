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
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "pcntl_alarm" => {
            let seconds = eval_pcntl_required_arg(args, 0)?;
            let seconds = eval_int_value(seconds.value, values)?;
            values.int(elephc_pcntl::elephc_pcntl_alarm(seconds))?
        }
        "pcntl_daemon" => {
            let no_chdir = eval_pcntl_arg(args, 0)
                .map(|arg| values.truthy(arg.value))
                .transpose()?
                .unwrap_or(false);
            let no_close = eval_pcntl_arg(args, 1)
                .map(|arg| values.truthy(arg.value))
                .transpose()?
                .unwrap_or(false);
            values.bool_value(
                elephc_pcntl::elephc_pcntl_daemon(
                    libc::c_int::from(no_chdir),
                    libc::c_int::from(no_close),
                ) != 0,
            )?
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
        "pcntl_getpriority" => eval_pcntl_getpriority(args, context, values)?,
        "posix_setpgid" => {
            let process_id = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
            let process_group_id =
                eval_int_value(eval_pcntl_required_arg(args, 1)?.value, values)?;
            values.bool_value(
                elephc_pcntl::elephc_posix_setpgid(process_id, process_group_id) != 0,
            )?
        }
        "posix_setsid" => {
            if args.iter().any(Option::is_some) {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.int(elephc_pcntl::elephc_posix_setsid())?
        }
        "pcntl_setpriority" => eval_pcntl_setpriority(args, context, values)?,
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
        "pcntl_setcpuaffinity" => eval_pcntl_setcpuaffinity(args, context, values)?,
        #[cfg(target_os = "linux")]
        "pcntl_setns" => eval_pcntl_setns(args, context, values)?,
        #[cfg(target_os = "linux")]
        "pcntl_unshare" => eval_pcntl_unshare(args, context, values)?,
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
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 0), 0, values)?;
    let mode = eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0, values)?;
    eval_validate_priority_mode("pcntl_getpriority", 2, mode, context, values)?;
    eval_validate_darwin_thread_process_id(
        "pcntl_getpriority",
        1,
        2,
        process_id,
        mode,
        context,
        values,
    )?;
    let mut priority = 0;
    let success = unsafe {
        elephc_pcntl::elephc_pcntl_getpriority(process_id, mode as libc::c_int, &mut priority)
    };
    if success == 0 {
        values.warning(&elephc_pcntl::pcntl_last_error_warning(
            elephc_pcntl::PCNTL_WARNING_GETPRIORITY,
        ))?;
        values.bool_value(false)
    } else {
        values.int(i64::from(priority))
    }
}

/// Changes a process priority and returns the bridge success flag.
fn eval_pcntl_setpriority(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let priority = eval_int_value(eval_pcntl_required_arg(args, 0)?.value, values)?;
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0, values)?;
    let mode = eval_pcntl_optional_int(eval_pcntl_arg(args, 2), 0, values)?;
    eval_validate_priority_mode("pcntl_setpriority", 3, mode, context, values)?;
    eval_validate_darwin_thread_process_id(
        "pcntl_setpriority",
        2,
        3,
        process_id,
        mode,
        context,
        values,
    )?;
    let success = elephc_pcntl::elephc_pcntl_setpriority(
        priority as libc::c_int,
        process_id,
        mode as libc::c_int,
    ) != 0;
    if !success {
        values.warning(&elephc_pcntl::pcntl_last_error_warning(
            elephc_pcntl::PCNTL_WARNING_SETPRIORITY,
        ))?;
    }
    values.bool_value(success)
}

/// Enforces Darwin's current-thread-only process id rule for `PRIO_DARWIN_THREAD`.
fn eval_validate_darwin_thread_process_id(
    name: &str,
    argument: usize,
    mode_argument: usize,
    process_id: i64,
    mode: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    #[cfg(target_os = "macos")]
    if mode == 3 && process_id != 0 {
        let _: RuntimeCellHandle = eval_throw_builtin_value_error(
            &format!(
                "{name}(): Argument #{argument} ($process_id) must be 0 (zero) if PRIO_DARWIN_THREAD is provided as {} parameter",
                ordinal_parameter(mode_argument)
            ),
            context,
            values,
        )?;
        return Err(EvalStatus::RuntimeFatal);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (
        name,
        argument,
        mode_argument,
        process_id,
        mode,
        context,
        values,
    );
    Ok(())
}

/// Returns the ordinal word PHP uses for a one-based parameter position.
#[cfg(target_os = "macos")]
fn ordinal_parameter(position: usize) -> &'static str {
    match position {
        2 => "second",
        3 => "third",
        _ => "selected",
    }
}

/// Raises PHP's target-specific `ValueError` for an unsupported priority selector.
fn eval_validate_priority_mode(
    name: &str,
    argument: usize,
    mode: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    #[cfg(target_os = "macos")]
    let (valid, allowed) = (
        (0..=3).contains(&mode),
        "PRIO_PGRP, PRIO_USER, PRIO_PROCESS or PRIO_DARWIN_THREAD",
    );
    #[cfg(target_os = "linux")]
    let (valid, allowed) = (
        (0..=2).contains(&mode),
        "PRIO_PGRP, PRIO_USER, or PRIO_PROCESS",
    );
    if valid {
        return Ok(());
    }
    let _: RuntimeCellHandle = eval_throw_builtin_value_error(
        &format!("{name}(): Argument #{argument} ($mode) must be one of {allowed}"),
        context,
        values,
    )?;
    Ok(())
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
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_id = eval_pcntl_optional_int(eval_pcntl_arg(args, 0), 0, values)?;
    let cpus = eval_pcntl_int_array(
        eval_pcntl_required_arg(args, 1)?.value,
        "pcntl_setcpuaffinity",
        2,
        "cpu_ids",
        "CPU id",
        context,
        values,
    )?;
    let result = unsafe {
        elephc_pcntl::elephc_pcntl_setcpuaffinity(process_id, cpus.as_ptr(), cpus.len())
    };
    match result {
        1 => values.bool_value(true),
        -4..=-1 => eval_throw_builtin_value_error(
            &elephc_pcntl::pcntl_cpu_affinity_value_error(result, process_id),
            context,
            values,
        ),
        _ => {
            values.warning(&elephc_pcntl::pcntl_last_error_warning(
                elephc_pcntl::PCNTL_WARNING_CPU_AFFINITY,
            ))?;
            values.bool_value(false)
        }
    }
}

/// Joins the selected Linux process namespace.
#[cfg(target_os = "linux")]
fn eval_pcntl_setns(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let process_arg = eval_pcntl_arg(args, 0);
    let use_current_process = match process_arg {
        None => true,
        Some(arg) => values.is_null(arg.value)?,
    };
    let process_id = if use_current_process {
        i64::from(unsafe { libc::getpid() })
    } else {
        eval_pcntl_optional_int(process_arg, 0, values)?
    };
    let namespace_type =
        eval_pcntl_optional_int(eval_pcntl_arg(args, 1), 0x4000_0000, values)?;
    let result =
        elephc_pcntl::elephc_pcntl_setns(process_id, namespace_type as libc::c_int);
    match result {
        1 => values.bool_value(true),
        -1 => eval_throw_builtin_value_error(
            &format!(
                "pcntl_setns(): Argument #1 ($process_id) is not a valid process ({process_id})"
            ),
            context,
            values,
        ),
        -2 => eval_throw_builtin_value_error(
            &format!(
                "pcntl_setns(): Argument #1 ($process_id) process no longer available ({process_id})"
            ),
            context,
            values,
        ),
        -3 => eval_throw_builtin_value_error(
            &format!(
                "pcntl_setns(): Argument #2 ($nstype) is an invalid nstype ({namespace_type})"
            ),
            context,
            values,
        ),
        _ => {
            values.warning(&elephc_pcntl::pcntl_last_error_warning(
                elephc_pcntl::PCNTL_WARNING_SETNS,
            ))?;
            values.bool_value(false)
        }
    }
}

/// Disassociates the requested Linux execution contexts.
#[cfg(target_os = "linux")]
fn eval_pcntl_unshare(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = eval_pcntl_required_arg(args, 0)?;
    let flags = eval_int_value(flags.value, values)?;
    match elephc_pcntl::elephc_pcntl_unshare(flags as libc::c_int) {
        1 => values.bool_value(true),
        -1 => eval_throw_builtin_value_error(
            "pcntl_unshare(): Argument #1 ($flags) must be a combination of CLONE_* flags, or at least one flag is unsupported by the kernel",
            context,
            values,
        ),
        _ => {
            values.warning(&elephc_pcntl::pcntl_last_error_warning(
                elephc_pcntl::PCNTL_WARNING_UNSHARE,
            ))?;
            values.bool_value(false)
        }
    }
}
