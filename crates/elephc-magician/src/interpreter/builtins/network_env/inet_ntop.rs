//! Purpose:
//! Eval registry entry and implementation for `inet_ntop`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - The family is the input length, exactly as php-src decides it: 4 or 16, anything else
//!   is PHP false.
//! - IPv4 formatting delegates to `long2ip` so byte rendering stays aligned; IPv6 goes through
//!   the platform's own `inet_ntop(3)`, which owns the `::` compression and the embedded-IPv4
//!   spelling, as in the compiled `__rt_inet_ntop` helper (#1157).

use super::*;

eval_builtin! {
    contract: "inet_ntop",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `inet_ntop($binary)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_inet_ntop(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [binary] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let binary = eval_expr(binary, context, scope, values)?;
    eval_inet_ntop_result(binary, values)
}

/// Renders a 4-byte IPv4 or 16-byte IPv6 binary address as text, or PHP false.
pub(in crate::interpreter) fn eval_inet_ntop_result(
    binary: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(binary)?;
    if let [a, b, c, d] = bytes.as_slice() {
        let ip = u32::from_be_bytes([*a, *b, *c, *d]);
        return values.string(&eval_format_ipv4(ip));
    }
    match eval_inet_ntop_ipv6(&bytes) {
        Some(text) => values.string_bytes_value(&text),
        None => values.bool_value(false),
    }
}

/// Renders sixteen network-order bytes through the platform's `inet_ntop(3)`.
fn eval_inet_ntop_ipv6(address: &[u8]) -> Option<Vec<u8>> {
    let address: &[u8; 16] = address.try_into().ok()?;
    // INET6_ADDRSTRLEN (46) is the longest IPv6 presentation form plus its NUL.
    let mut text = [0 as libc::c_char; 46];
    // SAFETY: `address` holds the sixteen bytes AF_INET6 reads, and `text` is as large as the
    // size passed, so the platform formatter cannot write past it.
    let rendered = unsafe {
        libc_inet_ntop(
            libc::AF_INET6,
            address.as_ptr().cast::<libc::c_void>(),
            text.as_mut_ptr(),
            text.len() as libc::socklen_t,
        )
    };
    if rendered.is_null() {
        return None;
    }
    // SAFETY: a non-null result is `text`, NUL-terminated by the formatter.
    Some(unsafe { CStr::from_ptr(text.as_ptr()) }.to_bytes().to_vec())
}
