//! Purpose:
//! Declarative eval registry entry for `urlencode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing URL encode hook.

eval_builtin! {
    name: "urlencode",
    area: String,
    params: [string],
    direct: UrlEncode,
    values: UrlEncode,
}

use super::super::super::*;

/// Evaluates PHP `urlencode(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_urlencode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urlencode::eval_builtin_url_encode_named("urlencode", args, context, scope, values)
}

/// Applies PHP `urlencode(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_urlencode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urlencode::eval_url_encode_named_result("urlencode", value, values)
}

/// Evaluates a named PHP URL encoder over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_url_encode_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_url_encode_named_result(name, value, values)
}

/// Percent-encodes one PHP string using query-style or RFC 3986 URL rules.
pub(in crate::interpreter) fn eval_url_encode_named_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in bytes {
        if eval_url_encode_keeps_byte(name, byte)? {
            output.push(byte);
        } else if name == "urlencode" && byte == b' ' {
            output.push(b'+');
        } else {
            output.push(b'%');
            output.push(HEX[(byte >> 4) as usize]);
            output.push(HEX[(byte & 0x0f) as usize]);
        }
    }
    values.string_bytes_value(&output)
}

/// Returns whether a byte remains unescaped for the selected PHP URL encoder.
pub(in crate::interpreter) fn eval_url_encode_keeps_byte(
    name: &str,
    byte: u8,
) -> Result<bool, EvalStatus> {
    let common = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.');
    match name {
        "urlencode" => Ok(common),
        "rawurlencode" => Ok(common || byte == b'~'),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
