//! Purpose:
//! Declarative eval registry entry for `bin2hex`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing hex encode hook.

eval_builtin! {
    name: "bin2hex",
    area: String,
    params: [string],
    direct: Bin2Hex,
    values: Bin2Hex,
}

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
