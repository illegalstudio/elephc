//! Purpose:
//! Eval registry entry and implementation for `getmypid`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Eval runs inside the compiled program's own process, so the id it reports is the same one
//!   the compiled `getmypid()` reads through libc `getpid()`.

use super::*;

eval_builtin! {
    contract: "getmypid",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `getmypid()` over its (empty) eval argument list.
pub(in crate::interpreter) fn eval_builtin_getmypid(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_getmypid_result(values)
}

/// Returns the current process id as a PHP integer.
pub(in crate::interpreter) fn eval_getmypid_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(i64::from(std::process::id()))
}
