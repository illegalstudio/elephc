//! Purpose:
//! Eval registry entry and implementation for `preg_last_error_msg`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Messages come from `elephc_builtin_contract::pcre_errors`, shared with the AOT table; an unknown code
//!   reads as "No error", like the AOT helper.

use super::super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_last_error_msg",
    area: Regex,
    direct: PregLastErrorMsg,
    values: PregLastErrorMsg,
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

/// Returns PHP's message for the last eval regex error code.
pub(in crate::interpreter) fn eval_preg_last_error_msg_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(elephc_builtin_contract::pcre_errors::preg_error_message(
        eval_preg_last_error_code(),
    ))
}
