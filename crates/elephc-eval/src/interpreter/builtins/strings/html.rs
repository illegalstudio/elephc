//! Purpose:
//! HTML entity encode/decode builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and string bytes are obtained through `RuntimeValueOps`.

use super::super::super::*;

/// Evaluates eval HTML entity encode/decode builtins over one string expression.
pub(in crate::interpreter) fn eval_builtin_html_entity(
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
    eval_html_entity_result(name, value, values)
}

/// Applies the eval-supported HTML entity transform for one PHP string value.
pub(in crate::interpreter) fn eval_html_entity_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "htmlspecialchars" | "htmlentities" => eval_htmlspecialchars_result(value, values),
        "html_entity_decode" => eval_html_entity_decode_result(value, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Encodes the HTML-special byte characters covered by elephc's static helper.
pub(in crate::interpreter) fn eval_htmlspecialchars_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            b'&' => output.extend_from_slice(b"&amp;"),
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'"' => output.extend_from_slice(b"&quot;"),
            b'\'' => output.extend_from_slice(b"&#039;"),
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}

/// Decodes one pass of the HTML entities emitted by the eval/static encoders.
pub(in crate::interpreter) fn eval_html_entity_decode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'&' {
            if let Some((decoded, width)) = eval_html_entity_at(&bytes[index..]) {
                output.push(decoded);
                index += width;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    values.string_bytes_value(&output)
}

/// Returns the decoded byte and consumed width for one supported HTML entity.
pub(in crate::interpreter) fn eval_html_entity_at(bytes: &[u8]) -> Option<(u8, usize)> {
    for (entity, decoded) in [
        (b"&lt;".as_slice(), b'<'),
        (b"&gt;".as_slice(), b'>'),
        (b"&quot;".as_slice(), b'"'),
        (b"&#039;".as_slice(), b'\''),
        (b"&#39;".as_slice(), b'\''),
        (b"&amp;".as_slice(), b'&'),
    ] {
        if bytes.starts_with(entity) {
            return Some((decoded, entity.len()));
        }
    }
    None
}
