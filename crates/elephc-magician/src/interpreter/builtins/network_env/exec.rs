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
        "exec" | "shell_exec" => values.string_bytes_value(&output),
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
    eval_shell_run_with(
        || {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        },
        |child| child.wait_with_output().map(|output| output.stdout),
        std::thread::sleep,
    )
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
