//! Purpose:
//! Eval registry entry and implementation for `long2ip`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - The IPv4 formatter is owned here and reused by `inet_ntop`.

use super::*;

eval_builtin! {
    name: "long2ip",
    area: NetworkEnv,
    params: [ip],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `long2ip($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_long2ip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_long2ip_result(ip, values)
}

/// Formats one 32-bit IPv4 integer as a dotted-quad string.
pub(in crate::interpreter) fn eval_long2ip_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip = eval_int_value(ip, values)? as u32;
    values.string(&eval_format_ipv4(ip))
}


/// Formats one packed IPv4 integer into dotted-quad text.
pub(in crate::interpreter) fn eval_format_ipv4(ip: u32) -> String {
    let [a, b, c, d] = ip.to_be_bytes();
    format!("{}.{}.{}.{}", a, b, c, d)
}
