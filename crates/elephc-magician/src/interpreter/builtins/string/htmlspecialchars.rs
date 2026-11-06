//! Purpose:
//! Declarative eval registry entry for `htmlspecialchars`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the HTML entity hook.
//! - `$flags` selects the escaped quotes and the single-quote entity exactly like the compiled
//!   `__rt_htmlspecialchars` helper (#1464); `$encoding` is evaluated and not applied, as there.

eval_builtin! {
    contract: "htmlspecialchars",
    area: String,
    direct: HtmlEntity,
    values: HtmlEntity,
}

use super::super::super::*;
/// Evaluates PHP `htmlspecialchars(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_htmlspecialchars(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::htmlspecialchars::eval_builtin_html_entity_named("htmlspecialchars", args, context, scope, values)
}

/// PHP's `ENT_HTML_QUOTE_SINGLE` bit: escape `'`.
const ENT_HTML_QUOTE_SINGLE: i64 = 1;
/// PHP's `ENT_COMPAT` bit: escape `"`.
const ENT_COMPAT: i64 = 2;
/// PHP's doctype bits; any of `ENT_XML1` (16), `ENT_XHTML` (32), or `ENT_HTML5` (48) spells
/// the single quote `&apos;` instead of HTML 4.01's `&#039;`.
const ENT_DOCTYPE_MASK: i64 = 48;
/// PHP's default `$flags`: `ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML401`.
pub(in crate::interpreter) const ENT_DEFAULT_FLAGS: i64 = 11;

/// Evaluates a named HTML entity encode/decode builtin over one string expression.
///
/// The encoders accept optional `$flags` and `$encoding` arguments. Every argument is evaluated
/// in source order; `$flags` then selects the quote handling and `$encoding` has no effect,
/// exactly as in the compiled helper.
pub(in crate::interpreter) fn eval_builtin_html_entity_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let accepts_options = matches!(name, "htmlspecialchars" | "htmlentities");
    let (value, flags) = match args {
        [value] => (value, None),
        [value, flags] | [value, flags, _] if accepts_options => (value, Some(flags)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let value = eval_expr(value, context, scope, values)?;
    let flags = match flags {
        Some(flags) => Some(eval_expr(flags, context, scope, values)?),
        None => None,
    };
    if let [_, _, encoding] = args {
        eval_expr(encoding, context, scope, values)?;
    }
    let flags = match flags {
        Some(flags) => eval_int_value(flags, values)?,
        None => ENT_DEFAULT_FLAGS,
    };
    eval_html_entity_named_result(name, value, flags, values)
}

/// Applies the eval-supported HTML entity transform for one PHP string value.
///
/// `flags` only affects the encoders; `html_entity_decode()` ignores it.
pub(in crate::interpreter) fn eval_html_entity_named_result(
    name: &str,
    value: RuntimeCellHandle,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "htmlspecialchars" | "htmlentities" => eval_htmlspecialchars_result(value, flags, values),
        "html_entity_decode" => eval_html_entity_decode_value_result(value, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Encodes the HTML-special byte characters the compiled helper encodes, under `$flags`.
///
/// `"` is escaped only under `ENT_COMPAT` and `'` only under `ENT_HTML_QUOTE_SINGLE`, so
/// `ENT_NOQUOTES` leaves both literal; the single quote is `&#039;` under HTML 4.01 and `&apos;`
/// under the XML1, XHTML and HTML5 doctypes.
pub(in crate::interpreter) fn eval_htmlspecialchars_result(
    value: RuntimeCellHandle,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let single_quote: &[u8] = if flags & ENT_DOCTYPE_MASK == 0 { b"&#039;" } else { b"&apos;" };
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            b'&' => output.extend_from_slice(b"&amp;"),
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'"' if flags & ENT_COMPAT != 0 => output.extend_from_slice(b"&quot;"),
            b'\'' if flags & ENT_HTML_QUOTE_SINGLE != 0 => output.extend_from_slice(single_quote),
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}

/// Decodes one pass of the HTML entities emitted by the eval/static encoders.
pub(in crate::interpreter) fn eval_html_entity_decode_value_result(
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
