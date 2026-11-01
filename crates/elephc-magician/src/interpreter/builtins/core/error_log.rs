//! Purpose:
//! Declarative eval registry entry and implementation for `error_log`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - message_type 0 (default) and every type this embedding has no real handler for (1 = email,
//!   4 = SAPI logging handler) all write to the DEFAULT channel: measured `php -n` 8.5.6 on CLI
//!   with no `error_log` ini configured, `error_log("hi")` writes `hi\n` to fd 2 (stderr) and
//!   returns `true`; an unrecognized type falls back to the same channel too (measured:
//!   `error_log("x", 5)` still writes to stderr and returns `true`). This is measurably the ONLY
//!   channel that TRUNCATES the message at an embedded NUL byte (measured:
//!   `error_log("a\0b")` writes just `a\n`) -- a real SAPI-logger/syslog behavior php's other
//!   destinations do not share.
//! - message_type 3 (append to file) writes the RAW message bytes with NO added newline and NO
//!   NUL truncation (measured: `error_log("a\0b", 3, $path)` writes the full 3 bytes `a\0b` to
//!   the file), and an empty destination is a catchable `ValueError` with php's exact wording
//!   `"Path must not be empty"` (no `error_log():` prefix -- confirmed by direct measurement,
//!   this is one of the rare internal-function errors that does not name the function).
//! - message_type 1 (email) is NOT implemented: this crate has no `mail()` sender. It falls back
//!   to the default stderr channel rather than silently discarding the message, which is a
//!   deliberate divergence from php (which would attempt delivery and warn on failure) -- stated
//!   here because Symfony's own `Logger.php:101` call site never uses it (always type 0).
//! - `additional_headers` (the 4th parameter) is accepted for arity but never read; it only
//!   matters to the unimplemented email path.

use super::super::super::*;

eval_builtin! {
    contract: "error_log",
    area: Core,
    direct: Core,
    values: Core,
}

/// Dispatches a direct eval `error_log()` call, preserving PHP source argument order.
pub(in crate::interpreter) fn eval_builtin_error_log(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return eval_throw_argument_count_error(
            "error_log() expects at least 1 argument, 0 given",
            context,
            values,
        );
    }
    if args.len() > 4 {
        return eval_throw_argument_count_error(
            &format!("error_log() expects at most 4 arguments, {} given", args.len()),
            context,
            values,
        );
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_error_log_result(&evaluated, context, values)
}

/// Dispatches an evaluated-argument `error_log()` call (named args, spread, callable dispatch).
pub(in crate::interpreter) fn eval_error_log_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        return eval_throw_argument_count_error(
            "error_log() expects at least 1 argument, 0 given",
            context,
            values,
        );
    }
    if evaluated_args.len() > 4 {
        return eval_throw_argument_count_error(
            &format!(
                "error_log() expects at most 4 arguments, {} given",
                evaluated_args.len()
            ),
            context,
            values,
        );
    }
    eval_error_log_result(evaluated_args, context, values)
}

/// Writes one message to the channel selected by `$message_type`, returning php's `bool` result.
fn eval_error_log_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let message = eval_require_string_arg(args[0], "error_log", 1, "message", context, values)?;
    let message_type = match args.get(1) {
        Some(&value) => eval_require_int_arg(value, "error_log", 2, "message_type", context, values)?,
        None => 0,
    };
    let destination = match args.get(2) {
        Some(&value) => {
            eval_require_string_arg(value, "error_log", 3, "destination", context, values)?
        }
        None => Vec::new(),
    };
    // args.get(3) ("additional_headers") is accepted for arity but unused (see module doc).

    if message_type == 3 {
        if destination.is_empty() {
            return eval_throw_builtin_value_error("Path must not be empty", context, values);
        }
        let path = String::from_utf8_lossy(&destination).into_owned();
        if eval_error_log_append_file(&path, &message).is_err() {
            return values.bool_value(false);
        }
        return values.bool_value(true);
    }

    // Every other type (0 = default, 1 = email -- unimplemented, 4 = SAPI handler -- no SAPI
    // here, or an unrecognized number) falls back to the default stderr channel.
    let mut out = eval_error_log_truncate_at_nul(&message).to_vec();
    out.push(b'\n');
    values.error_log_write_stderr(&out)?;
    values.bool_value(true)
}

/// Truncates a message at its first embedded NUL byte, matching the default channel's measured
/// (syslog/SAPI-logger-style) C-string behavior -- the file channel does NOT do this.
fn eval_error_log_truncate_at_nul(message: &[u8]) -> &[u8] {
    match message.iter().position(|&byte| byte == 0) {
        Some(index) => &message[..index],
        None => message,
    }
}

/// Appends the raw message bytes to a file, creating it if absent.
fn eval_error_log_append_file(path: &str, message: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(message)
}
