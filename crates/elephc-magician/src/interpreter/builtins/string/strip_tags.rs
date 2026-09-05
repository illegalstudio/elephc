//! Purpose:
//! Declarative eval registry entry for `strip_tags`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Implements PHP 8.5 `strip_tags(string $string, array|string|null $allowed_tags = null)`.
//! - HTML comments and PHP tags are always stripped; they cannot be allow-listed.
//! - The state machine is a port of php-src `php_strip_tags_ex` / `php_tag_find`.

eval_builtin! {
    contract: "strip_tags",
    area: String,
    direct: StripTags,
    values: StripTags,
}

use super::super::super::*;

/// Evaluates PHP `strip_tags(...)` over one string and an optional allow-list.
pub(in crate::interpreter) fn eval_builtin_strip_tags(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_strip_tags_result(value, None, context, values)
        }
        [value, allowed] => {
            let value = eval_expr(value, context, scope, values)?;
            let allowed = eval_expr(allowed, context, scope, values)?;
            eval_strip_tags_result(value, Some(allowed), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `strip_tags()` to already-evaluated arguments.
pub(in crate::interpreter) fn eval_strip_tags_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [value] => eval_strip_tags_result(*value, None, context, values),
        [value, allowed] => eval_strip_tags_result(*value, Some(*allowed), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Strips HTML/PHP tags from one string using an optional PHP 8.5 allow-list.
pub(in crate::interpreter) fn eval_strip_tags_result(
    value: RuntimeCellHandle,
    allowed: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let input = values.string_bytes(value)?;
    let allow = match allowed {
        None => None,
        Some(allowed) if values.is_null(allowed)? => None,
        Some(allowed) if values.is_array_like(allowed)? => {
            Some(eval_strip_tags_allow_from_array(allowed, values)?)
        }
        Some(allowed) if values.type_tag(allowed)? == EVAL_TAG_STRING => {
            let bytes = values.string_bytes(allowed)?;
            if bytes.is_empty() {
                None
            } else {
                Some(bytes)
            }
        }
        Some(allowed) => {
            let given = eval_strip_tags_type_name(values.type_tag(allowed)?);
            return eval_throw_type_error(
                &format!(
                    "strip_tags(): Argument #2 ($allowed_tags) must be of type array|string|null, {given} given"
                ),
                context,
                values,
            );
        }
    };
    values.string_bytes_value(&php_strip_tags(&input, allow.as_deref()))
}

/// Builds PHP's `<tag><tag>` allow string from an array of tag names.
fn eval_strip_tags_allow_from_array(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut allow = Vec::new();
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let tag = values.array_get(array, key)?;
        let tag = values.cast_string(tag)?;
        let tag = values.string_bytes(tag)?;
        allow.push(b'<');
        allow.extend_from_slice(&tag);
        allow.push(b'>');
    }
    Ok(allow)
}

/// Returns PHP's TypeError type label for an eval Mixed tag.
fn eval_strip_tags_type_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "int",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "bool",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "null",
        _ => "mixed",
    }
}

/// Returns whether `byte` is a C-locale `isspace` byte used by php-src `strip_tags`.
fn php_strip_tags_is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | b'\x0c' | b'\r')
}

/// Normalizes one collected tag and reports whether it appears in `allow`.
///
/// Mirrors php-src `php_tag_find`: lowercase, drop leading/trailing whitespace inside
/// the tag, ignore a closing `/` after `<` or before `>`, then `strstr` the allow set.
fn php_tag_find(tag: &[u8], allow: &[u8]) -> bool {
    if tag.is_empty() {
        return false;
    }
    let mut norm = Vec::with_capacity(tag.len() + 1);
    let mut state = 0;
    let mut index = 0;
    while index < tag.len() {
        let byte = tag[index].to_ascii_lowercase();
        match byte {
            b'<' => norm.push(b'<'),
            b'>' => break,
            _ if php_strip_tags_is_space(byte) => {
                if state == 1 {
                    break;
                }
            }
            _ => {
                if state == 0 {
                    state = 1;
                }
                let prev = if index > 0 { tag[index - 1] } else { 0 };
                let next = tag.get(index + 1).copied().unwrap_or(0);
                if byte != b'/' || (prev != b'<' && next != b'>') {
                    norm.push(byte);
                }
            }
        }
        index += 1;
    }
    norm.push(b'>');
    allow.windows(norm.len()).any(|window| window == norm)
}

/// Strips HTML and PHP tags using PHP 8.5 `php_strip_tags_ex(..., allow_tag_spaces=0)`.
fn php_strip_tags(input: &[u8], allow: Option<&[u8]>) -> Vec<u8> {
    let allow = allow.and_then(|bytes| {
        if bytes.is_empty() {
            None
        } else {
            Some(bytes.to_ascii_lowercase())
        }
    });
    let allow = allow.as_deref();

    let mut output = Vec::with_capacity(input.len());
    let mut tag = Vec::new();
    let mut index = 0;
    let mut state = 0;
    let mut in_q: u8 = 0;
    let mut depth: i32 = 0;
    let mut br: i32 = 0;
    let mut lc: u8 = 0;
    let mut is_xml = false;

    while index < input.len() {
        match state {
            0 => {
                let byte = input[index];
                match byte {
                    0 => {}
                    b'<' if in_q != 0 => {}
                    b'<' => {
                        let next = input.get(index + 1).copied().unwrap_or(0);
                        if php_strip_tags_is_space(next) {
                            output.push(b'<');
                        } else {
                            lc = b'<';
                            state = 1;
                            if allow.is_some() {
                                tag.clear();
                                tag.push(b'<');
                            }
                            index += 1;
                            continue;
                        }
                    }
                    b'>' if depth > 0 => depth -= 1,
                    b'>' if in_q != 0 => {}
                    _ => output.push(byte),
                }
                index += 1;
            }
            1 => {
                let byte = input[index];
                match byte {
                    0 => {}
                    b'<' if in_q != 0 => {}
                    b'<' => {
                        let next = input.get(index + 1).copied().unwrap_or(0);
                        if php_strip_tags_is_space(next) {
                            php_strip_tags_collect(allow, &mut tag, byte);
                        } else {
                            depth += 1;
                        }
                    }
                    b'>' if depth > 0 => depth -= 1,
                    b'>' if in_q != 0 => {}
                    b'>' if is_xml && index > 0 && input[index - 1] == b'-' => {}
                    b'>' => {
                        lc = b'>';
                        in_q = 0;
                        state = 0;
                        is_xml = false;
                        if let Some(allow) = allow {
                            tag.push(b'>');
                            if php_tag_find(&tag, allow) {
                                output.extend_from_slice(&tag);
                            }
                            tag.clear();
                        }
                        index += 1;
                        continue;
                    }
                    b'"' | b'\'' => {
                        if index != 0 && (in_q == 0 || in_q == byte) {
                            in_q = if in_q == 0 { byte } else { 0 };
                        }
                        php_strip_tags_collect(allow, &mut tag, byte);
                    }
                    b'!' if index > 0 && input[index - 1] == b'<' => {
                        state = 3;
                        lc = byte;
                        index += 1;
                        continue;
                    }
                    b'?' if index > 0 && input[index - 1] == b'<' => {
                        br = 0;
                        state = 2;
                        index += 1;
                        continue;
                    }
                    _ => php_strip_tags_collect(allow, &mut tag, byte),
                }
                index += 1;
            }
            2 => {
                let byte = input[index];
                match byte {
                    b'(' if lc != b'"' && lc != b'\'' => {
                        lc = b'(';
                        br += 1;
                    }
                    b')' if lc != b'"' && lc != b'\'' => {
                        lc = b')';
                        br -= 1;
                    }
                    b'>' if depth > 0 => depth -= 1,
                    b'>' if in_q != 0 => {}
                    b'>'
                        if br == 0
                            && index > 0
                            && lc != b'"'
                            && input[index - 1] == b'?' =>
                    {
                        in_q = 0;
                        state = 0;
                        tag.clear();
                        index += 1;
                        continue;
                    }
                    b'"' | b'\'' if index > 0 && input[index - 1] != b'\\' => {
                        if lc == byte {
                            lc = 0;
                        } else if lc != b'\\' {
                            lc = byte;
                        }
                        if index != 0 && (in_q == 0 || in_q == byte) {
                            in_q = if in_q == 0 { byte } else { 0 };
                        }
                    }
                    b'l' | b'L'
                        if index >= 4
                            && matches!(input[index - 1], b'm' | b'M')
                            && matches!(input[index - 2], b'x' | b'X')
                            && input[index - 3] == b'?'
                            && input[index - 4] == b'<' =>
                    {
                        state = 1;
                        is_xml = true;
                        index += 1;
                        continue;
                    }
                    _ => {}
                }
                index += 1;
            }
            3 => {
                let byte = input[index];
                match byte {
                    b'>' if depth > 0 => depth -= 1,
                    b'>' if in_q != 0 => {}
                    b'>' => {
                        in_q = 0;
                        state = 0;
                        tag.clear();
                        index += 1;
                        continue;
                    }
                    b'"' | b'\'' if index != 0 && input[index - 1] != b'\\' && (in_q == 0 || in_q == byte) => {
                        in_q = if in_q == 0 { byte } else { 0 };
                    }
                    b'-' if index >= 2 && input[index - 1] == b'-' && input[index - 2] == b'!' => {
                        state = 4;
                        index += 1;
                        continue;
                    }
                    b'E' | b'e'
                        if index > 6
                            && matches!(input[index - 1], b'p' | b'P')
                            && matches!(input[index - 2], b'y' | b'Y')
                            && matches!(input[index - 3], b't' | b'T')
                            && matches!(input[index - 4], b'c' | b'C')
                            && matches!(input[index - 5], b'o' | b'O')
                            && matches!(input[index - 6], b'd' | b'D') =>
                    {
                        state = 1;
                        index += 1;
                        continue;
                    }
                    _ => {}
                }
                index += 1;
            }
            _ => {
                let byte = input[index];
                if byte == b'>'
                    && in_q == 0
                    && index >= 2
                    && input[index - 1] == b'-'
                    && input[index - 2] == b'-'
                {
                    in_q = 0;
                    state = 0;
                    tag.clear();
                }
                index += 1;
            }
        }
    }
    output
}

/// Appends one collected tag byte when an allow-list is active.
fn php_strip_tags_collect(allow: Option<&[u8]>, tag: &mut Vec<u8>, byte: u8) {
    if allow.is_some() {
        tag.push(byte);
    }
}

#[cfg(test)]
mod tests {
    use super::{php_strip_tags, php_tag_find};

    /// Verifies the default path strips tags, comments, PHP, and NUL bytes.
    #[test]
    fn php_strip_tags_strips_html_php_comments_and_nuls() {
        assert_eq!(
            php_strip_tags(b"<p>Hello <b>World</b></p>", None),
            b"Hello World"
        );
        assert_eq!(php_strip_tags(b"plain", None), b"plain");
        assert_eq!(php_strip_tags(b"a<!-- hide -->b", None), b"ab");
        assert_eq!(php_strip_tags(b"a<?php echo 1; ?>b", None), b"ab");
        assert_eq!(php_strip_tags(b"a\0b<c>", None), b"ab");
        assert_eq!(php_strip_tags(b"hello<b", None), b"hello");
        assert_eq!(php_strip_tags(b"1 < 2", None), b"1 < 2");
        assert_eq!(
            php_strip_tags(b"<p>Hi</p>", None) == b"<p>Hi</p>",
            false
        );
    }

    /// Verifies string and implicit array-form allow-lists keep matching tags.
    #[test]
    fn php_strip_tags_keeps_allowed_tags() {
        assert_eq!(
            php_strip_tags(b"<p>Hello <b>World</b></p>", Some(b"<p>")),
            b"<p>Hello World</p>"
        );
        assert_eq!(
            php_strip_tags(b"<BR/>hi</br>", Some(b"<br>")),
            b"<BR/>hi</br>"
        );
        assert_eq!(
            php_strip_tags(b"<p class=\"x\">Hi</p>", Some(b"<p>")),
            b"<p class=\"x\">Hi</p>"
        );
        assert!(php_tag_find(b"<BR/>", b"<br>"));
        assert!(php_tag_find(b"</p>", b"<p>"));
        assert!(!php_tag_find(b"<b>", b"<p>"));
    }
}
