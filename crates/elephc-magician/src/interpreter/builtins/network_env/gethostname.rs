//! Purpose:
//! Eval registry entry and implementation for `gethostname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - The libc hostname buffer is stack-owned and copied into a PHP string.

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

/// Reads the current host name through libc and returns an empty string on failure.
pub(in crate::interpreter) fn eval_gethostname_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut buffer = [0 as libc::c_char; 256];
    let status = unsafe {
        // libc writes at most buffer.len() bytes into this stack buffer.
        libc::gethostname(buffer.as_mut_ptr(), buffer.len())
    };
    if status != 0 {
        return values.string("");
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    let hostname = buffer[..length]
        .iter()
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    values.string_bytes_value(&hostname)
}
