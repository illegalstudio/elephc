//! Purpose:
//! Implements IPv4 conversion eval builtins such as `ip2long`, `long2ip`,
//! `inet_pton`, and `inet_ntop`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` re-exports.
//!
//! Key details:
//! - The supported eval subset is IPv4-only and returns PHP false for malformed
//!   addresses or binary payloads.

use super::super::super::*;
use super::super::*;

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

/// Evaluates PHP `ip2long($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ip2long(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_ip2long_result(ip, values)
}

/// Parses a dotted-quad IPv4 string into an integer or PHP false.
pub(in crate::interpreter) fn eval_ip2long_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    match eval_parse_ipv4(&bytes) {
        Some(ip) => values.int(i64::from(ip)),
        None => values.bool_value(false),
    }
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

/// Packs a dotted-quad IPv4 string into four network-order bytes or PHP false.
pub(in crate::interpreter) fn eval_inet_pton_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    let Some(ip) = eval_parse_ipv4(&bytes) else {
        return values.bool_value(false);
    };
    values.string_bytes_value(&ip.to_be_bytes())
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

/// Renders a four-byte IPv4 string as dotted-quad text or PHP false.
pub(in crate::interpreter) fn eval_inet_ntop_result(
    binary: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(binary)?;
    let [a, b, c, d] = bytes.as_slice() else {
        return values.bool_value(false);
    };
    let ip = u32::from_be_bytes([*a, *b, *c, *d]);
    values.string(&eval_format_ipv4(ip))
}

/// Parses exactly four decimal IPv4 octets separated by dots.
pub(in crate::interpreter) fn eval_parse_ipv4(bytes: &[u8]) -> Option<u32> {
    let mut octets = [0_u8; 4];
    let mut position = 0_usize;
    let mut index = 0_usize;

    while index < 4 {
        if position >= bytes.len() {
            return None;
        }
        let start = position;
        let mut value = 0_u16;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            value = value
                .checked_mul(10)?
                .checked_add(u16::from(bytes[position] - b'0'))?;
            position += 1;
            if position - start > 3 || value > 255 {
                return None;
            }
        }
        if position == start {
            return None;
        }
        octets[index] = value as u8;
        index += 1;
        if index == 4 {
            return (position == bytes.len()).then(|| u32::from_be_bytes(octets));
        }
        if bytes.get(position).copied() != Some(b'.') {
            return None;
        }
        position += 1;
    }
    None
}

/// Formats one packed IPv4 integer into dotted-quad text.
pub(in crate::interpreter) fn eval_format_ipv4(ip: u32) -> String {
    let [a, b, c, d] = ip.to_be_bytes();
    format!("{}.{}.{}.{}", a, b, c, d)
}
