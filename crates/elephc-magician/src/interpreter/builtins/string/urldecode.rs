//! Purpose:
//! Declarative eval registry entry for `urldecode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing URL decode hook.

eval_builtin! {
    name: "urldecode",
    area: String,
    params: [string],
    direct: UrlDecode,
    values: UrlDecode,
}

use super::super::super::*;

/// Evaluates PHP `urldecode(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_urldecode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urldecode::eval_builtin_url_decode_named("urldecode", args, context, scope, values)
}

/// Applies PHP `urldecode(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_urldecode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urldecode::eval_url_decode_named_result("urldecode", value, values)
}

/// Evaluates a named PHP URL decoder over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_url_decode_named(
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
    eval_url_decode_named_result(name, value, values)
}

/// Decodes `%XX` sequences and optionally maps `+` to space for `urldecode()`.
pub(in crate::interpreter) fn eval_url_decode_named_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let plus_to_space = match name {
        "urldecode" => true,
        "rawurldecode" => false,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'+' && plus_to_space {
            output.push(b' ');
            index += 1;
        } else if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                super::hex2bin::eval_hex_nibble(bytes[index + 1]),
                super::hex2bin::eval_hex_nibble(bytes[index + 2]),
            ) {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
            output.push(bytes[index]);
            index += 1;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    values.string_bytes_value(&output)
}
