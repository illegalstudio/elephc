//! Purpose:
//! Eval registry entry and implementation for `preg_last_error_msg`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Messages match PHP 8.5 and the AOT `_preg_err_msg_*` table; an unknown code
//!   reads as "No error", like the AOT helper.

use super::super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_last_error_msg",
    area: Regex,
    direct: PregLastErrorMsg,
    values: PregLastErrorMsg,
}

/// PHP 8.5's `preg_last_error_msg()` strings, indexed by `PREG_*_ERROR` code.
const EVAL_PREG_ERROR_MESSAGES: &[&str] = &[
    "No error",
    "Internal error",
    "Backtrack limit exhausted",
    "Recursion limit exhausted",
    "Malformed UTF-8 characters, possibly incorrectly encoded",
    "The offset did not correspond to the beginning of a valid UTF-8 code point",
    "JIT stack limit exhausted",
];

/// Evaluates PHP `preg_last_error_msg()` with no eval arguments.
pub(in crate::interpreter) fn eval_builtin_preg_last_error_msg(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_preg_last_error_msg_result(values)
}

/// Returns PHP's message for the last eval regex error code.
pub(in crate::interpreter) fn eval_preg_last_error_msg_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let message = usize::try_from(eval_preg_last_error_code())
        .ok()
        .and_then(|code| EVAL_PREG_ERROR_MESSAGES.get(code))
        .copied()
        .unwrap_or(EVAL_PREG_ERROR_MESSAGES[0]);
    values.string(message)
}
