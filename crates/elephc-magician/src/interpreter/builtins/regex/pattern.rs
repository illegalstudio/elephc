//! Purpose:
//! Parses PHP delimited preg patterns and compiles them into PCRE2 regexes.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex` entrypoint modules.
//!
//! Key details:
//! - Only eval-supported modifiers are accepted; unsupported delimiters or
//!   malformed patterns produce runtime fatal status.

use super::super::super::*;

/// Compiles one eval PCRE-style delimited pattern into a PCRE2 regex.
pub(in crate::interpreter) fn eval_preg_regex(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Regex, EvalStatus> {
    let pattern = values.string_bytes(pattern)?;
    let (body, modifiers) = eval_preg_pattern_parts(&pattern)?;
    Regex::compile(&body, modifiers)
}

/// Splits a PHP delimited regex into body bytes and supported modifiers.
pub(in crate::interpreter) fn eval_preg_pattern_parts(
    pattern: &[u8],
) -> Result<(Vec<u8>, EvalPregModifiers), EvalStatus> {
    if pattern.len() < 2 || pattern[0].is_ascii_alphanumeric() || pattern[0].is_ascii_whitespace() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let delimiter = pattern[0];
    if delimiter == b'\\' {
        return Err(EvalStatus::RuntimeFatal);
    }
    let closing = eval_preg_closing_delimiter(delimiter);
    let close_index =
        eval_preg_find_closing_delimiter(pattern, closing).ok_or(EvalStatus::RuntimeFatal)?;
    let body = eval_preg_unescape_delimiter(&pattern[1..close_index], delimiter, closing);
    let modifiers = eval_preg_modifiers(&pattern[close_index + 1..])?;
    Ok((body, modifiers))
}

/// Returns the closing regex delimiter for PHP's paired delimiter forms.
pub(in crate::interpreter) fn eval_preg_closing_delimiter(delimiter: u8) -> u8 {
    match delimiter {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        _ => delimiter,
    }
}

/// Finds the first unescaped closing regex delimiter.
pub(in crate::interpreter) fn eval_preg_find_closing_delimiter(
    pattern: &[u8],
    closing: u8,
) -> Option<usize> {
    let mut escaped = false;
    for (index, byte) in pattern.iter().copied().enumerate().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == closing {
            return Some(index);
        }
    }
    None
}

/// Removes escapes that only protect the PHP regex delimiter from pattern stripping.
pub(in crate::interpreter) fn eval_preg_unescape_delimiter(
    body: &[u8],
    delimiter: u8,
    closing: u8,
) -> Vec<u8> {
    let mut result = Vec::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        if body[index] == b'\\'
            && index + 1 < body.len()
            && matches!(body[index + 1], byte if byte == delimiter || byte == closing)
        {
            result.push(body[index + 1]);
            index += 2;
        } else {
            result.push(body[index]);
            index += 1;
        }
    }
    result
}

/// Parses eval-supported PHP regex modifiers.
pub(in crate::interpreter) fn eval_preg_modifiers(
    modifiers: &[u8],
) -> Result<EvalPregModifiers, EvalStatus> {
    let mut parsed = EvalPregModifiers::default();
    for modifier in modifiers {
        match *modifier {
            b'i' => parsed.case_insensitive = true,
            b'm' => parsed.multi_line = true,
            b's' => parsed.dot_matches_new_line = true,
            b'U' => parsed.swap_greed = true,
            b'u' => parsed.unicode = true,
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(parsed)
}
