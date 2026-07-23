//! Purpose:
//! Eval registry entry and implementation for `gethostbyaddr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - OS resolver storage is copied before any subsequent resolver lookup can overwrite it.

use super::*;

eval_builtin! {
    name: "gethostbyaddr",
    area: NetworkEnv,
    params: [ip],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `gethostbyaddr($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gethostbyaddr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_gethostbyaddr_result(ip, values)
}

/// Reverse-resolves one IPv4 address, returns the input on miss, or PHP false when malformed.
pub(in crate::interpreter) fn eval_gethostbyaddr_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip_bytes = values.string_bytes(ip)?;
    let ip_text = String::from_utf8_lossy(&ip_bytes);
    let Ok(ipv4) = ip_text.parse::<std::net::Ipv4Addr>() else {
        return values.bool_value(false);
    };
    let resolved = eval_reverse_ipv4_name(ipv4.octets());
    match resolved {
        Some(name) if !name.is_empty() => values.string_bytes_value(&name),
        _ => values.string(ip_text.as_ref()),
    }
}
