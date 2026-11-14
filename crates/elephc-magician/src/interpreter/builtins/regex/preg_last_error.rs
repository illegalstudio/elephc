//! Purpose:
//! Eval registry entry and implementation for `preg_last_error`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Reads the thread-local eval regex error slot that every eval `preg_*` call
//!   clears when it compiles its pattern (see `engine`).

use super::super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_last_error",
    area: Regex,
    direct: PregLastError,
    values: PregLastError,
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

/// Returns the `PREG_*_ERROR` code of the last eval regex call.
pub(in crate::interpreter) fn eval_preg_last_error_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(eval_preg_last_error_code())
}
