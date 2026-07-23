//! Purpose:
//! Eval registry entry and implementation for `gethostname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - The OS hostname buffer is copied into a PHP string before returning.

use super::*;

eval_builtin! {
    name: "gethostname",
    area: NetworkEnv,
    params: [],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `gethostname()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gethostname(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_gethostname_result(values)
}

/// Reads the current host name through the platform API and returns an empty string on failure.
pub(in crate::interpreter) fn eval_gethostname_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_os_hostname() {
        Some(hostname) => values.string_bytes_value(&hostname),
        None => values.string(""),
    }
}
