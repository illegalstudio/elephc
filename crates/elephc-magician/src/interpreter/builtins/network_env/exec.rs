//! Purpose:
//! Eval registry entry and implementation for `exec` plus shared shell runner helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - `shell_exec`, `system`, and `passthru` call the runner owned by this file.
//! - Spawn failures and output failures remain distinct from successful empty output.
//! - Only a failed spawn may be retried; a child that started is never executed again.

use std::io::ErrorKind;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::*;

eval_builtin! {
    contract: "exec",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates `exec($command)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_exec(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_process_command("exec", args, context, scope, values)
}

/// Evaluates already materialized `exec()` command arguments.
pub(in crate::interpreter) fn eval_exec_result(
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_process_command_result("exec", command, values)
}

/// Splits command output into php's `exec()` lines: each ends at `\n`, loses its trailing
/// whitespace (`isspace`: space, `\t`, `\n`, `\v`, `\f`, `\r`), and a final newline opens no
/// further empty line.
pub(in crate::interpreter) fn eval_exec_lines(output: &[u8]) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    let mut rest = output;
    while !rest.is_empty() {
        let (line, tail) = match rest.iter().position(|byte| *byte == b'\n') {
            Some(end) => (&rest[..end], &rest[end + 1..]),
            None => (rest, &rest[rest.len()..]),
        };
        let mut end = line.len();
        while end > 0 && matches!(line[end - 1], b' ' | b'\t' | b'\r' | 0x0b | 0x0c) {
            end -= 1;
        }
        lines.push(line[..end].to_vec());
        rest = tail;
    }
    lines
}

/// Evaluates `exec($command, &$output, &$result_code)` with writable reference targets.
///
/// Every output line is appended to `$output` (an existing array keeps its elements), the exit
/// status lands in `$result_code`, and the last line is the result, as in php's `php_exec`.
pub(in crate::interpreter) fn eval_builtin_exec_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated, context, _, values| {
        let (bound, _) =
            bind_evaluated_ref_builtin_args(&["command", "output", "result_code"], &evaluated, false)?;
        let command = required_evaluated_ref_arg(&bound, 0)?.value;
        let output = optional_evaluated_ref_arg(&bound, 1);
        let status = optional_evaluated_ref_arg(&bound, 2);
        let command = eval_shell_command_string(command, values)?;
        let (outcome, exit_code) = eval_shell_command_output_with_status(&command);
        let bytes = match outcome {
            EvalShellOutcome::Ran(bytes) => bytes,
            other => return eval_process_outcome_result("exec", other, values),
        };
        let lines = eval_exec_lines(&bytes);
        let last = lines.last().cloned().unwrap_or_default();
        if let Some(argument) = output {
            if let Some(target) = argument.ref_target.clone() {
                let combined = eval_exec_append_lines(argument.value, &lines, values)?;
                eval_write_direct_ref_target(
                    &target,
                    combined,
                    context,
                    values,
                    Some(ScopeCellOwnership::Owned),
                )?;
            }
        }
        if let Some(argument) = status {
            if let Some(target) = argument.ref_target.clone() {
                let code = values.int(exit_code)?;
                eval_write_direct_ref_target(
                    &target,
                    code,
                    context,
                    values,
                    Some(ScopeCellOwnership::Owned),
                )?;
            }
        }
        values.string_bytes_value(&last)
    })
}

/// Builds the `$output` array: a copy of the current value when it is an array, plus `lines`.
fn eval_exec_append_lines(
    current: RuntimeCellHandle,
    lines: &[Vec<u8>],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = if matches!(values.type_tag(current)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        values.array_clone_shallow(current)?
    } else {
        values.array_new(lines.len())?
    };
    let mut next = 0i64;
    for position in 0..values.array_len(array)? {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? == EVAL_TAG_INT {
            next = next.max(eval_int_value(key, values)?.saturating_add(1));
        }
        values.release(key)?;
    }
    for line in lines {
        let key = values.int(next)?;
        let value = values.string_bytes_value(line)?;
        array = values.array_set(array, key, value)?;
        next += 1;
    }
    Ok(array)
}

/// Evaluates one eval process-control builtin over a command expression.
pub(in crate::interpreter) fn eval_builtin_process_command(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [command] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let command = eval_expr(command, context, scope, values)?;
    eval_process_command_result(name, command, values)
}

/// Evaluates one already materialized process-control command argument.
pub(in crate::interpreter) fn eval_process_command_result(
    name: &str,
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let command = eval_shell_command_string(command, values)?;
    eval_process_outcome_result(name, eval_shell_command_output(&command), values)
}

/// Distinguishes successful output from errors before and after a child starts.
pub(in crate::interpreter) enum EvalShellOutcome {
    /// The command ran. These are its stdout bytes, which may legitimately be empty.
    Ran(Vec<u8>),
    /// The process could not be created, so the command never ran at all.
    SpawnFailed(std::io::Error),
    /// The process started, but its output or exit status could not be collected.
    OutputFailed(std::io::Error),
}

/// Maps shell output to the builtin result, preserving operating-system errors in warnings.
pub(in crate::interpreter) fn eval_process_outcome_result(
    name: &str,
    outcome: EvalShellOutcome,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(name, "exec" | "shell_exec" | "system" | "passthru") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let output = match outcome {
        EvalShellOutcome::Ran(output) => output,
        EvalShellOutcome::SpawnFailed(error) => {
            values.warning(&format!("{name}(): Unable to start process: {error}"))?;
            return values.bool_value(false);
        }
        EvalShellOutcome::OutputFailed(error) => {
            values.warning(&format!("{name}(): Unable to collect process output: {error}"))?;
            return values.bool_value(false);
        }
    };
    match name {
        // php's `exec()` answers the last line of output with trailing whitespace stripped.
        "exec" => {
            let last = eval_exec_lines(&output).pop().unwrap_or_default();
            values.string_bytes_value(&last)
        }
        "shell_exec" => values.string_bytes_value(&output),
        "system" => {
            eval_echo_process_output(&output, values)?;
            values.string("")
        }
        "passthru" => {
            eval_echo_process_output(&output, values)?;
            values.null()
        }
        // Unreachable: the name was validated above. Kept rather than `unreachable!()` so a
        // future name added to one list and not the other cannot panic through eval.
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Converts a PHP command cell into the host shell string accepted by `Command`.
fn eval_shell_command_string(
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let command = values.string_bytes(command)?;
    Ok(String::from_utf8_lossy(&command).into_owned())
}

/// Maximum spawn attempts for one command. Bounded so a genuinely exhausted process table
/// still terminates instead of spinning.
const EVAL_SHELL_SPAWN_ATTEMPTS: u32 = 5;

/// Spawns the shell with captured output and collects it once, outside the retry boundary.
fn eval_shell_command_output(command: &str) -> EvalShellOutcome {
    eval_shell_command_output_with_status(command).0
}

/// Like `eval_shell_command_output`, and also answers the child's exit code (`-1` when a signal
/// ended it or it never ran), which `exec()`'s `$result_code` reports.
fn eval_shell_command_output_with_status(command: &str) -> (EvalShellOutcome, i64) {
    let exit_code = std::cell::Cell::new(-1_i64);
    let outcome = eval_shell_run_with(
        || {
            let _signals = elephc_process_context::SpawnPermit::new();
            Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        },
        |child| {
            child.wait_with_output().map(|output| {
                exit_code.set(i64::from(output.status.code().unwrap_or(-1)));
                output.stdout
            })
        },
        std::thread::sleep,
    );
    (outcome, exit_code.get())
}

/// Retries only transient spawn errors, with injectable operations for deterministic tests.
fn eval_shell_run_with<Child>(
    mut spawn: impl FnMut() -> std::io::Result<Child>,
    collect: impl FnOnce(Child) -> std::io::Result<Vec<u8>>,
    mut sleep: impl FnMut(Duration),
) -> EvalShellOutcome {
    let mut backoff = Duration::from_millis(1);
    for attempt in 1..=EVAL_SHELL_SPAWN_ATTEMPTS {
        match spawn() {
            Ok(child) => {
                return match collect(child) {
                    Ok(output) => EvalShellOutcome::Ran(output),
                    Err(error) => EvalShellOutcome::OutputFailed(error),
                };
            }
            Err(error) => {
                if attempt == EVAL_SHELL_SPAWN_ATTEMPTS || !eval_shell_spawn_is_transient(&error) {
                    return EvalShellOutcome::SpawnFailed(error);
                }
                sleep(backoff);
                backoff *= 2;
            }
        }
    }
    EvalShellOutcome::SpawnFailed(std::io::Error::from(ErrorKind::WouldBlock))
}

/// Recognizes temporary process or memory exhaustion without retrying persistent errors.
fn eval_shell_spawn_is_transient(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::OutOfMemory)
        || matches!(error.raw_os_error(), Some(libc::EAGAIN) | Some(libc::ENOMEM))
}

/// Echoes captured process output through the eval runtime value hooks.
fn eval_echo_process_output(
    output: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if output.is_empty() {
        return Ok(());
    }
    let output = values.string_bytes_value(output)?;
    values.echo(output)
}

#[cfg(test)]
mod tests {
    use super::{eval_shell_run_with, eval_shell_spawn_is_transient, EvalShellOutcome};
    use std::io::{Error, ErrorKind};
    use std::time::Duration;

    /// A shell launched from evaluated PHP receives the original callback signal mask.
    #[test]
    fn deferred_web_callbacks_spawn_terminable_eval_shells() {
        let _callback = elephc_process_context::Deferral::new();
        let outcome = super::eval_shell_command_output("kill -TERM $$; printf survived");
        match outcome {
            EvalShellOutcome::Ran(output) => assert!(output.is_empty(), "child survived SIGTERM"),
            _ => panic!("eval shell failed to launch"),
        }
    }

    /// Retries failed spawns with bounded backoff and collects a successful child exactly once.
    #[test]
    fn transient_spawns_retry_before_collecting_output() {
        let mut attempts = 0;
        let mut collected = 0;
        let mut delays = Vec::new();
        let outcome = eval_shell_run_with(
            || {
                attempts += 1;
                match attempts {
                    1 => Err(Error::from_raw_os_error(libc::EAGAIN)),
                    2 => Err(Error::from_raw_os_error(libc::ENOMEM)),
                    _ => Ok(42),
                }
            },
            |child| {
                assert_eq!(child, 42);
                collected += 1;
                Ok(b"once".to_vec())
            },
            |delay| delays.push(delay),
        );
        assert!(matches!(outcome, EvalShellOutcome::Ran(bytes) if bytes == b"once"));
        assert_eq!((attempts, collected), (3, 1));
        assert_eq!(delays, [Duration::from_millis(1), Duration::from_millis(2)]);
    }

    /// Exhausted and permanent spawn failures preserve errno without collecting a child.
    #[test]
    fn failed_spawns_keep_the_original_error_and_retry_bound() {
        for (errno, expected_attempts) in [(libc::EAGAIN, 5), (libc::EACCES, 1)] {
            let mut attempts = 0;
            let mut delays = Vec::new();
            let outcome = eval_shell_run_with(
                || {
                    attempts += 1;
                    Err::<(), _>(Error::from_raw_os_error(errno))
                },
                |_| panic!("a failed spawn has no child to collect"),
                |delay| delays.push(delay),
            );
            assert!(matches!(outcome, EvalShellOutcome::SpawnFailed(error)
                if error.raw_os_error() == Some(errno)));
            assert_eq!(attempts, expected_attempts);
            assert_eq!(delays.len(), expected_attempts - 1);
            assert!(delays.iter().sum::<Duration>() <= Duration::from_millis(15));
        }
    }

    /// A transient-looking output error must never start a second child or sleep.
    #[test]
    fn collecting_output_cannot_reexecute_a_started_command() {
        for errno in [libc::EAGAIN, libc::ENOMEM, libc::ECHILD] {
            let mut attempts = 0;
            let outcome = eval_shell_run_with(
                || {
                    attempts += 1;
                    Ok(())
                },
                |_| Err(Error::from_raw_os_error(errno)),
                |_| panic!("output errors must not enter the spawn backoff"),
            );
            assert!(matches!(outcome, EvalShellOutcome::OutputFailed(error)
                if error.raw_os_error() == Some(errno)));
            assert_eq!(attempts, 1);
        }
    }

    /// The two errors a loaded kernel raises when it declines to create a process must be
    /// retryable — this is the classifier that turns a CI flake into a completed command.
    ///
    /// Asserted through BOTH spellings the runner accepts, because the mapping from `errno`
    /// to `ErrorKind` is a std detail that has moved between Rust releases and the
    /// `curl-feature-contract` job compiles with whatever `stable` is on the day it runs.
    #[test]
    fn transient_spawn_errors_are_retryable_through_both_spellings() {
        for raw in [libc::EAGAIN, libc::ENOMEM] {
            assert!(
                eval_shell_spawn_is_transient(&Error::from_raw_os_error(raw)),
                "raw os error {raw} must be retryable"
            );
        }
        for kind in [ErrorKind::WouldBlock, ErrorKind::OutOfMemory] {
            assert!(
                eval_shell_spawn_is_transient(&Error::new(kind, "synthetic")),
                "{kind:?} must be retryable"
            );
        }
    }

    /// NEGATIVE CONTROL: a standing condition must NOT be retried. Retrying `ENOENT` for a
    /// missing `/bin/sh` would turn every such call into five sleeps and the same answer.
    #[test]
    fn standing_spawn_errors_are_not_retryable() {
        for raw in [libc::ENOENT, libc::EACCES, libc::E2BIG] {
            assert!(
                !eval_shell_spawn_is_transient(&Error::from_raw_os_error(raw)),
                "raw os error {raw} must not be retryable"
            );
        }
        assert!(!eval_shell_spawn_is_transient(&Error::new(
            ErrorKind::PermissionDenied,
            "synthetic"
        )));
    }
}
