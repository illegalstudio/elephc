//! Purpose:
//! Eval registry entry and implementation for `putenv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Assignments mutate the host process environment for the current eval process.
//! - PHP's syntax guard raises a catchable `ValueError` before host environment APIs run.
//! - The result mirrors the compiled `putenv()` lowering (#911), which hands libc the argument
//!   (read up to its first NUL byte, since the environment holds C strings): an argument
//!   containing `=` takes the set form, anything else the unset form, and `true` means libc
//!   accepted the change. Eval applies the change through Rust's `std::env` setters, which
//!   take the lock Rust's environment readers and process spawning take, and refuses exactly
//!   the arguments libc would refuse before any setter runs.

use super::*;

eval_builtin! {
    contract: "putenv",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `putenv($assignment)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_putenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [assignment] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let assignment = eval_expr(assignment, context, scope, values)?;
    eval_putenv_result(assignment, context, values)
}

/// Validates and applies one `putenv()` assignment to the host environment.
pub(in crate::interpreter) fn eval_putenv_result(
    assignment: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let assignment = values.string_bytes(assignment)?;
    if assignment.is_empty() || assignment[0] == b'=' {
        return eval_throw_builtin_value_error(
            "putenv(): Argument #1 ($assignment) must have a valid syntax",
            context,
            values,
        );
    }
    values.bool_value(eval_putenv_host(&assignment))
}

/// Applies one syntax-checked assignment and reports the status the compiled program's libc
/// call would report for it.
///
/// libc reads the compiled runtime's copy only up to its first NUL byte, while the `=` that
/// selects the set form is searched in every byte, as the compiled lowering searches it; both
/// are reproduced here. The change itself goes through `std::env::set_var`/`remove_var`, so it
/// is serialized with Rust's environment lock rather than racing a concurrent `std::env` read
/// or process spawn on another thread. Those setters panic where libc answers `EINVAL`, so
/// every such case is answered first:
/// - an empty name (the argument starts with NUL): both `putenv(3)` and `unsetenv(3)` refuse;
/// - a set form whose `=` sits after that NUL, which libc receives as a bare name: glibc and
///   musl `putenv(3)` unset it, Apple's refuses it.
#[cfg(unix)]
fn eval_putenv_host(assignment: &[u8]) -> bool {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let text = assignment.split(|byte| *byte == 0).next().unwrap_or_default();
    if text.is_empty() {
        return false;
    }
    let separator = text.iter().position(|byte| *byte == b'=');
    match separator {
        Some(separator) => {
            // The syntax guard rejected a leading `=`, so the name is never empty, and the
            // first `=` ends it: neither side holds a NUL or the name an `=`.
            std::env::set_var(
                OsStr::from_bytes(&text[..separator]),
                OsStr::from_bytes(&text[separator + 1..]),
            );
            true
        }
        None if assignment.contains(&b'=') && cfg!(target_vendor = "apple") => false,
        None => {
            std::env::remove_var(OsStr::from_bytes(text));
            true
        }
    }
}

/// Applies one assignment through Windows' Unicode environment boundary.
///
/// PHP strings can contain arbitrary bytes, while Rust's Windows environment API requires
/// Unicode. Invalid UTF-8 is refused rather than rewritten to a different variable name/value.
#[cfg(windows)]
fn eval_putenv_host(assignment: &[u8]) -> bool {
    let text = assignment.split(|byte| *byte == 0).next().unwrap_or_default();
    if text.is_empty() {
        return false;
    }
    let Ok(text) = std::str::from_utf8(text) else {
        return false;
    };
    match text.split_once('=') {
        Some((name, value)) => {
            std::env::set_var(name, value);
            true
        }
        None => {
            std::env::remove_var(text);
            true
        }
    }
}
