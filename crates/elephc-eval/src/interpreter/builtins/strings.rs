//! Purpose:
//! String, hash, ctype, SPL registry, and stream-introspection builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

/// Evaluates PHP's `sqrt(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_sqrt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.sqrt(value)
}

/// Evaluates PHP's `strrev(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_strrev(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.strrev(value)
}

/// Evaluates PHP's `chr(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_chr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_chr_result(value, values)
}

/// Converts one eval value to a PHP integer and returns the low byte as a string.
pub(in crate::interpreter) fn eval_chr_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_int_value(value, values)?;
    values.string_bytes_value(&[value as u8])
}

/// Evaluates PHP's `str_repeat(...)` over one eval expression pair.
pub(in crate::interpreter) fn eval_builtin_str_repeat(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, times] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let times = eval_expr(times, context, scope, values)?;
    eval_str_repeat_result(value, times, values)
}

/// Repeats one PHP string byte sequence according to a PHP-cast integer count.
pub(in crate::interpreter) fn eval_str_repeat_result(
    value: RuntimeCellHandle,
    times: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let times = eval_int_value(times, values)?;
    if times < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let times = usize::try_from(times).map_err(|_| EvalStatus::RuntimeFatal)?;
    let capacity = bytes
        .len()
        .checked_mul(times)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    for _ in 0..times {
        output.extend_from_slice(&bytes);
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `str_replace(...)` or `str_ireplace(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_str_replace(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [search, replace, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let search = eval_expr(search, context, scope, values)?;
    let replace = eval_expr(replace, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_str_replace_result(name, search, replace, subject, values)
}

/// Replaces every non-overlapping occurrence of a byte-string needle in a subject.
pub(in crate::interpreter) fn eval_str_replace_result(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let search = values.string_bytes(search)?;
    let replace = values.string_bytes(replace)?;
    let subject = values.string_bytes(subject)?;
    if search.is_empty() {
        return values.string_bytes_value(&subject);
    }

    let mut output = Vec::with_capacity(subject.len());
    let mut start = 0;
    while let Some(found) = eval_find_replace_match(name, &subject, &search, start)? {
        output.extend_from_slice(&subject[start..found]);
        output.extend_from_slice(&replace);
        start = found + search.len();
    }
    output.extend_from_slice(&subject[start..]);
    values.string_bytes_value(&output)
}

/// Finds the next replacement match using case-sensitive or ASCII-insensitive comparison.
pub(in crate::interpreter) fn eval_find_replace_match(
    name: &str,
    subject: &[u8],
    search: &[u8],
    start: usize,
) -> Result<Option<usize>, EvalStatus> {
    match name {
        "str_replace" => Ok(eval_find_subslice(subject, search, start)),
        "str_ireplace" => Ok(subject
            .get(start..)
            .and_then(|tail| {
                tail.windows(search.len())
                    .position(|window| window.eq_ignore_ascii_case(search))
            })
            .map(|position| position + start)),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `str_pad(...)` over a string, target length, pad string, and pad mode.
pub(in crate::interpreter) fn eval_builtin_str_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_pad_result(value, length, None, None, values)
        }
        [value, length, pad_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), None, values)
        }
        [value, length, pad_string, pad_type] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            let pad_type = eval_expr(pad_type, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), Some(pad_type), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Pads one byte string to a PHP target length using cyclic pad bytes.
pub(in crate::interpreter) fn eval_str_pad_result(
    value: RuntimeCellHandle,
    length: RuntimeCellHandle,
    pad_string: Option<RuntimeCellHandle>,
    pad_type: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let target_length = eval_int_value(length, values)?;
    let Ok(target_length) = usize::try_from(target_length) else {
        return values.string_bytes_value(&bytes);
    };
    if target_length <= bytes.len() {
        return values.string_bytes_value(&bytes);
    }

    let pad_string = match pad_string {
        Some(pad_string) => values.string_bytes(pad_string)?,
        None => b" ".to_vec(),
    };
    if pad_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let pad_type = match pad_type {
        Some(pad_type) => eval_int_value(pad_type, values)?,
        None => 1,
    };
    let (left_pad, right_pad) = eval_str_pad_sides(target_length - bytes.len(), pad_type)?;
    let capacity = bytes
        .len()
        .checked_add(left_pad)
        .and_then(|size| size.checked_add(right_pad))
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    eval_append_repeated_pad(&mut output, &pad_string, left_pad);
    output.extend_from_slice(&bytes);
    eval_append_repeated_pad(&mut output, &pad_string, right_pad);
    values.string_bytes_value(&output)
}

/// Splits a `str_pad()` pad budget into left and right byte counts.
pub(in crate::interpreter) fn eval_str_pad_sides(
    pad_budget: usize,
    pad_type: i64,
) -> Result<(usize, usize), EvalStatus> {
    match pad_type {
        0 => Ok((pad_budget, 0)),
        1 => Ok((0, pad_budget)),
        2 => Ok((pad_budget / 2, pad_budget - (pad_budget / 2))),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Appends `count` bytes by cycling through the provided non-empty pad string.
pub(in crate::interpreter) fn eval_append_repeated_pad(
    output: &mut Vec<u8>,
    pad_string: &[u8],
    count: usize,
) {
    for index in 0..count {
        output.push(pad_string[index % pad_string.len()]);
    }
}

/// Evaluates PHP `str_split(...)` over one string and optional chunk length.
pub(in crate::interpreter) fn eval_builtin_str_split(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_str_split_result(value, None, values)
        }
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_split_result(value, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Splits one byte string into indexed string chunks using PHP `str_split()` rules.
pub(in crate::interpreter) fn eval_str_split_result(
    value: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let length = match length {
        Some(length) => eval_int_value(length, values)?,
        None => 1,
    };
    if length <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut result = values.array_new(0)?;
    for (index, chunk) in bytes.chunks(length).enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string_bytes_value(chunk)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP's `nl2br(...)` over one eval expression and optional XHTML flag.
pub(in crate::interpreter) fn eval_builtin_nl2br(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_nl2br_result(value, true, values)
        }
        [value, use_xhtml] => {
            let value = eval_expr(value, context, scope, values)?;
            let use_xhtml = eval_expr(use_xhtml, context, scope, values)?;
            let use_xhtml = values.truthy(use_xhtml)?;
            eval_nl2br_result(value, use_xhtml, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Inserts an HTML line break before each PHP newline sequence while preserving bytes.
pub(in crate::interpreter) fn eval_nl2br_result(
    value: RuntimeCellHandle,
    use_xhtml: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let br = if use_xhtml {
        b"<br />".as_slice()
    } else {
        b"<br>".as_slice()
    };
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\r' || byte == b'\n' {
            output.extend_from_slice(br);
            output.push(byte);
            if index + 1 < bytes.len()
                && ((byte == b'\r' && bytes[index + 1] == b'\n')
                    || (byte == b'\n' && bytes[index + 1] == b'\r'))
            {
                output.push(bytes[index + 1]);
                index += 2;
                continue;
            }
        } else {
            output.push(byte);
        }
        index += 1;
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `substr(...)` over one eval string, offset, and optional length.
pub(in crate::interpreter) fn eval_builtin_substr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_result(value, offset, None, values)
        }
        [value, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_result(value, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Slices a PHP byte string using PHP `substr()` offset and length rules.
pub(in crate::interpreter) fn eval_substr_result(
    value: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(0)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let end = end.max(start);
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string_bytes_value(&bytes[start..end])
}

/// Evaluates PHP's `substr_replace(...)` over eval scalar byte strings.
pub(in crate::interpreter) fn eval_builtin_substr_replace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, replace, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, None, values)
        }
        [value, replace, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Replaces the byte range selected by PHP `substr_replace()` scalar rules.
pub(in crate::interpreter) fn eval_substr_replace_result(
    value: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let replacement = values.string_bytes(replace)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(start)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(bytes.len() + replacement.len());
    output.extend_from_slice(&bytes[..start]);
    output.extend_from_slice(&replacement);
    output.extend_from_slice(&bytes[end..]);
    values.string_bytes_value(&output)
}

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

/// Evaluates PHP URL encode builtins over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_url_encode(
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
    eval_url_encode_result(name, value, values)
}

/// Percent-encodes one PHP string using query-style or RFC 3986 URL rules.
pub(in crate::interpreter) fn eval_url_encode_result(
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

/// Evaluates PHP URL decode builtins over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_url_decode(
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
    eval_url_decode_result(name, value, values)
}

/// Decodes `%XX` sequences and optionally maps `+` to space for `urldecode()`.
pub(in crate::interpreter) fn eval_url_decode_result(
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
                eval_hex_nibble(bytes[index + 1]),
                eval_hex_nibble(bytes[index + 2]),
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

/// Evaluates PHP `ctype_*` predicates over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype(
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
    eval_ctype_result(name, value, values)
}

/// Returns the PHP boolean result for one ASCII `ctype_*` byte-string check.
pub(in crate::interpreter) fn eval_ctype_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut matches = !bytes.is_empty();
    for byte in bytes {
        if !eval_ctype_byte_matches(name, byte)? {
            matches = false;
            break;
        }
    }
    values.bool_value(matches)
}

/// Checks one byte against the selected PHP ASCII character class.
pub(in crate::interpreter) fn eval_ctype_byte_matches(
    name: &str,
    byte: u8,
) -> Result<bool, EvalStatus> {
    match name {
        "ctype_alpha" => Ok(byte.is_ascii_alphabetic()),
        "ctype_digit" => Ok(byte.is_ascii_digit()),
        "ctype_alnum" => Ok(byte.is_ascii_alphanumeric()),
        "ctype_space" => Ok(matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `crc32(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_crc32(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_crc32_result(value, values)
}

/// Computes PHP's non-negative CRC-32 integer over one converted byte string.
pub(in crate::interpreter) fn eval_crc32_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(eval_crc32_bytes(&bytes)))
}

/// Evaluates one-shot PHP hash digest builtins over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_one_shot(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_hash_one_shot_result(name, &evaluated_args, values)
}

/// Computes the result for one-shot PHP hash digest builtins from evaluated args.
pub(in crate::interpreter) fn eval_hash_one_shot_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "md5" | "sha1" => {
            let (data, binary) = match evaluated_args {
                [data] => (*data, false),
                [data, binary] => (*data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(name.as_bytes(), &data, binary, values)
        }
        "hash" => {
            let (algo, data, binary) = match evaluated_args {
                [algo, data] => (*algo, *data, false),
                [algo, data, binary] => (*algo, *data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(&algo, &data, binary, values)
        }
        "hash_file" => {
            let (algo, filename, binary) = match evaluated_args {
                [algo, filename] => (*algo, *filename, false),
                [algo, filename, binary] => (*algo, *filename, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            eval_hash_file_result(algo, filename, binary, values)
        }
        "hash_hmac" => {
            let (algo, data, key, binary) = match evaluated_args {
                [algo, data, key] => (*algo, *data, *key, false),
                [algo, data, key, binary] => (*algo, *data, *key, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            let key = values.string_bytes(key)?;
            eval_hash_hmac_result(&algo, &data, &key, binary, values)
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Reads a local file and returns its PHP hash digest or false when it cannot be read.
pub(in crate::interpreter) fn eval_hash_file_result(
    algo: RuntimeCellHandle,
    filename: RuntimeCellHandle,
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let algo = values.string_bytes(algo)?;
    let path = eval_path_string(filename, values)?;
    match std::fs::read(path) {
        Ok(data) => eval_hash_digest_result(&algo, &data, binary, values),
        Err(_) => values.bool_value(false),
    }
}

/// Computes a one-shot raw digest and formats it as PHP hex or raw bytes.
pub(in crate::interpreter) fn eval_hash_digest_result(
    algo: &[u8],
    data: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hash(algo, data)?;
    eval_format_digest_result(&raw, binary, values)
}

/// Computes a one-shot raw HMAC digest and formats it as PHP hex or raw bytes.
pub(in crate::interpreter) fn eval_hash_hmac_result(
    algo: &[u8],
    data: &[u8],
    key: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hmac(algo, data, key)?;
    eval_format_digest_result(&raw, binary, values)
}

/// Calls the elephc-crypto one-shot hash ABI and returns the raw digest bytes.
pub(in crate::interpreter) fn eval_crypto_hash(
    algo: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hash(
            algo.as_ptr(),
            algo.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    eval_crypto_digest_bytes(len, &output)
}

/// Calls the elephc-crypto one-shot HMAC ABI and returns the raw digest bytes.
pub(in crate::interpreter) fn eval_crypto_hmac(
    algo: &[u8],
    data: &[u8],
    key: &[u8],
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hmac(
            algo.as_ptr(),
            algo.len(),
            key.as_ptr(),
            key.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    eval_crypto_digest_bytes(len, &output)
}

/// Converts a crypto ABI digest length into an owned digest byte vector.
pub(in crate::interpreter) fn eval_crypto_digest_bytes(
    len: isize,
    output: &[u8; 64],
) -> Result<Vec<u8>, EvalStatus> {
    let len = usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    if len > output.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(output[..len].to_vec())
}

/// Formats a raw digest using PHP's `$binary` flag convention.
pub(in crate::interpreter) fn eval_format_digest_result(
    raw: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if binary {
        return values.string_bytes_value(raw);
    }
    values.string(&eval_lower_hex_bytes(raw))
}

/// Evaluates PHP `hash_algos()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_hash_algos(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_hash_algos_result(values)
}

/// Builds the indexed array returned by eval `hash_algos()`.
pub(in crate::interpreter) fn eval_hash_algos_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_HASH_ALGOS, values)
}

/// Builds one indexed PHP array from a static string slice.
pub(in crate::interpreter) fn eval_static_string_array_result(
    items: &[&str],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(items.len())?;
    for (index, item) in items.iter().enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string(item)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `spl_classes()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_spl_classes(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_spl_classes_result(values)
}

/// Builds the static class-name list returned by eval `spl_classes()`.
pub(in crate::interpreter) fn eval_spl_classes_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_SPL_CLASS_NAMES, values)
}

/// Evaluates PHP stream introspection list builtins with no arguments.
pub(in crate::interpreter) fn eval_builtin_stream_introspection(
    name: &str,
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_introspection_result(name, values)
}

/// Builds the static list returned by one eval stream introspection builtin.
pub(in crate::interpreter) fn eval_stream_introspection_result(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let items = match name {
        "stream_get_filters" => EVAL_STREAM_FILTERS,
        "stream_get_transports" => EVAL_STREAM_TRANSPORTS,
        "stream_get_wrappers" => EVAL_STREAM_WRAPPERS,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_static_string_array_result(items, values)
}
