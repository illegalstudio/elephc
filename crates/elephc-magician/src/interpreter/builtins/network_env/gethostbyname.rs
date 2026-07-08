//! Purpose:
//! Eval registry entry and implementation for `gethostbyname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Failed lookups return the original hostname input.

use super::*;

eval_builtin! {
    name: "gethostbyname",
    area: NetworkEnv,
    params: [hostname],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `gethostbyname($hostname)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gethostbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hostname] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hostname = eval_expr(hostname, context, scope, values)?;
    eval_gethostbyname_result(hostname, values)
}

/// Resolves one host name to an IPv4 string, or returns the original input on failure.
pub(in crate::interpreter) fn eval_gethostbyname_result(
    hostname: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let hostname = values.string_bytes(hostname)?;
    let hostname = String::from_utf8_lossy(&hostname);
    if hostname.parse::<std::net::Ipv4Addr>().is_ok() {
        return values.string(hostname.as_ref());
    }
    let resolved = (hostname.as_ref(), 0_u16)
        .to_socket_addrs()
        .ok()
        .and_then(|addrs| {
            addrs
                .filter_map(|addr| match addr.ip() {
                    std::net::IpAddr::V4(ip) => Some(ip.to_string()),
                    std::net::IpAddr::V6(_) => None,
                })
                .next()
        });
    values.string(resolved.as_deref().unwrap_or_else(|| hostname.as_ref()))
}
