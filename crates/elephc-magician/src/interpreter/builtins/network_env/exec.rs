//! Purpose:
//! Eval registry entry and implementation for `exec` plus shared shell runner helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - `shell_exec`, `system`, and `passthru` call the runner owned by this file.
//! - A SPAWN FAILURE IS NOT AN EMPTY COMMAND. See [`EvalShellOutcome`] — this runner used to
//!   collapse the two, which is a real bug and was also a CI flake.

use std::io::ErrorKind;
use std::process::Command;
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

/// What one shell invocation actually did.
///
/// THE TWO ARMS USED TO BE ONE VALUE, and collapsing them was a genuine defect rather than a
/// simplification: the runner ended in `.output().map(|o| o.stdout).unwrap_or_default()`, so a
/// process that could not be created AT ALL produced the same empty byte string as a command
/// that ran and printed nothing. `exec()` then answered `""` for a command it never executed.
///
/// That is also how it showed up in CI. On a loaded runner `fork`/`posix_spawn` fails with
/// `EAGAIN`, and one `exec()` inside an otherwise-passing test silently returned `""` while
/// its siblings succeeded — reproduced here directly: 2560 concurrent
/// `/bin/sh -c "printf spread"` spawns under `ulimit -u 900` produced 1177 `EAGAIN` failures,
/// every one of which this code would have reported as "the command printed nothing".
pub(in crate::interpreter) enum EvalShellOutcome {
    /// The command ran. These are its stdout bytes, which may legitimately be empty.
    Ran(Vec<u8>),
    /// The process could not be created, so the command never ran at all.
    SpawnFailed,
}

/// Maps one shell outcome onto the PHP return value of the calling builtin.
///
/// Split out of [`eval_process_command_result`] so the failure arm is reachable from a test
/// without having to induce a real spawn failure — inducing one needs process-table pressure,
/// which is exactly the nondeterminism a regression test must not depend on.
///
/// `SpawnFailed` answers `false` for all four builtins, which is php-src's own documented
/// failure value (`php_exec` returning `FAILURE` ends in `RETURN_FALSE`). The `Ran` arm is
/// byte-for-byte the behaviour this file always had.
pub(in crate::interpreter) fn eval_process_outcome_result(
    name: &str,
    outcome: EvalShellOutcome,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(name, "exec" | "shell_exec" | "system" | "passthru") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let EvalShellOutcome::Ran(output) = outcome else {
        return values.bool_value(false);
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

/// Executes a shell command, reporting whether it ran at all.
///
/// RETRIES A TRANSIENT SPAWN FAILURE, and that is safe precisely because it is a SPAWN
/// failure: `EAGAIN`/`ENOMEM` from `posix_spawn` mean the child was never created, so nothing
/// can have run twice. (A command that runs and then fails is `Ran` with whatever it wrote —
/// never retried.) Backoff doubles from 1ms, so five attempts span ~15ms before giving up.
///
/// php-src does not retry, but php-src does not silently answer `""` either: it reports
/// failure. Retrying first is what keeps `exec()` meaning "run this command" on a loaded
/// machine, and [`EvalShellOutcome::SpawnFailed`] is the honest answer when it genuinely
/// cannot.
fn eval_shell_command_output(command: &str) -> EvalShellOutcome {
    let mut backoff = Duration::from_millis(1);
    for attempt in 1..=EVAL_SHELL_SPAWN_ATTEMPTS {
        match Command::new("/bin/sh").arg("-c").arg(command).output() {
            Ok(output) => return EvalShellOutcome::Ran(output.stdout),
            Err(error) => {
                if attempt == EVAL_SHELL_SPAWN_ATTEMPTS || !eval_shell_spawn_is_transient(&error) {
                    return EvalShellOutcome::SpawnFailed;
                }
                std::thread::sleep(backoff);
                backoff *= 2;
            }
        }
    }
    EvalShellOutcome::SpawnFailed
}

/// Returns whether a spawn error is worth retrying — i.e. the OS declined to create the
/// process right now, rather than refusing this command outright.
///
/// `EAGAIN` (process/thread limit) and `ENOMEM` are the two the kernel raises under pressure.
/// Everything else — `ENOENT` for a missing `/bin/sh`, `EACCES`, `E2BIG` — is a standing
/// condition that a retry cannot change, so it fails immediately.
///
/// Matched on the RAW OS ERROR as well as `ErrorKind`, because the `ErrorKind` spelling for
/// these has changed across Rust releases (`WouldBlock` for `EAGAIN`, `OutOfMemory` for
/// `ENOMEM` only since 1.53) and this must not silently stop retrying under a different
/// toolchain than the one it was written against.
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
    use super::eval_shell_spawn_is_transient;
    use std::io::{Error, ErrorKind};

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
