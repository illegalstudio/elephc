//! Purpose:
//! Hex encoding and decoding builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::scalars` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and all PHP coercions flow through `RuntimeValueOps`.

use super::super::super::*;

/// Evaluates PHP's `bin2hex(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_bin2hex(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_bin2hex_result(value, values)
}

/// Converts one eval value through PHP string conversion and returns lowercase hex bytes.
pub(in crate::interpreter) fn eval_bin2hex_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.string(&eval_lower_hex_bytes(&bytes))
}

/// Converts bytes to lowercase hexadecimal text.
pub(in crate::interpreter) fn eval_lower_hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Evaluates PHP's `hex2bin(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_hex2bin(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_hex2bin_result(value, values)
}

/// Converts one eval value through PHP string conversion and decodes hexadecimal bytes.
pub(in crate::interpreter) fn eval_hex2bin_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    if bytes.len() % 2 != 0 {
        values.warning(HEX2BIN_ODD_LENGTH_WARNING)?;
        return values.bool_value(false);
    }
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let Some(high) = eval_hex_nibble(pair[0]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        let Some(low) = eval_hex_nibble(pair[1]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        output.push((high << 4) | low);
    }
    values.string_bytes_value(&output)
}

/// Returns the four-bit value for one hexadecimal byte.
pub(in crate::interpreter) fn eval_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
