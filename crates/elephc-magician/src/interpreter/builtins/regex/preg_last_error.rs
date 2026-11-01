//! Purpose:
//! Eval registry entries and implementations for `preg_last_error` and `preg_last_error_msg`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The code lives in the PCRE2 shim, shared with compiled `preg_*` calls as php's is: an eval
//!   call reports the error of whichever regex call ran last, compiled or interpreted.

use super::super::super::*;

eval_builtin! {
    contract: "preg_last_error",
    area: Regex,
    direct: PregLastError,
    values: PregLastError,
}

eval_builtin! {
    contract: "preg_last_error_msg",
    area: Regex,
    direct: PregLastErrorMsg,
    values: PregLastErrorMsg,
}

/// Evaluates PHP `preg_last_error()` with no eval arguments.
pub(in crate::interpreter) fn eval_builtin_preg_last_error(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_preg_last_error_result(values)
}

/// Returns the last regex error code.
pub(in crate::interpreter) fn eval_preg_last_error_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(i64::from(crate::regex_provider::regex_last_error()))
}

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

/// Returns php's message for the last regex error code.
pub(in crate::interpreter) fn eval_preg_last_error_msg_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let code = crate::regex_provider::regex_last_error();
    values.string(crate::regex_provider::regex_last_error_msg(code))
}
