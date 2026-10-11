//! Purpose:
//! Eval registry entry and implementation for `inet_pton`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Parsing is the platform's own `inet_pton(3)`, as in the compiled `__rt_inet_pton` helper
//!   and in php-src (#1157): a `:` anywhere in the input selects `AF_INET6`, anything else
//!   `AF_INET`.
//! - The platform parser reads a C string, so it sees the input up to its first NUL byte,
//!   exactly the NUL-terminated copy the compiled helper hands it.
//! - Input longer than `EVAL_INET_MAX_ADDRESS_BYTES` is refused before parsing, the compiled
//!   helper's documented bound (issue #1160; see `docs/php/strings.md`).

use super::*;

eval_builtin! {
    contract: "inet_pton",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `inet_pton($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_inet_pton(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_inet_pton_result(ip, values)
}

/// The longest textual address `inet_pton()` accepts, not counting the NUL, shared with the
/// compiled helper's `MAX_ADDRESS_BYTES`.
const EVAL_INET_MAX_ADDRESS_BYTES: usize = 255;

/// Packs a textual IPv4 or IPv6 address into its 4- or 16-byte network-order form, or PHP false.
pub(in crate::interpreter) fn eval_inet_pton_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    match eval_inet_pton_bytes(&bytes) {
        Some(packed) => values.string_bytes_value(&packed),
        None => values.bool_value(false),
    }
}

/// Runs the platform parser for the family php-src selects, returning the packed bytes.
fn eval_inet_pton_bytes(address: &[u8]) -> Option<Vec<u8>> {
    if address.is_empty() || address.len() > EVAL_INET_MAX_ADDRESS_BYTES {
        return None;
    }
    let ipv6 = address.contains(&b':');
    let width = if ipv6 { 16 } else { 4 };
    let text = address.split(|byte| *byte == 0).next().unwrap_or_default();
    let text = CString::new(text).ok()?;
    let mut packed = [0_u8; 16];
    eval_os_inet_pton(&text, ipv6, &mut packed).then(|| packed[..width].to_vec())
}
