//! Purpose:
//! Parses PHP delimited preg patterns and compiles them into PCRE2 regexes.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex` entrypoint modules.
//!
//! Key details:
//! - Only eval-supported modifiers are accepted. Other preg builtins report
//!   malformed patterns as a runtime fatal. `preg_replace_callback()` separates
//!   invalid patterns, which return null, from valid modifiers it does not map.
//!   Space, LF, and CR in the modifier suffix are ignored. `x` is extended mode.

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

/// Distinguishes an invalid callback pattern from a modifier this runtime does not map.
pub(in crate::interpreter) enum EvalPregCallbackPattern {
    /// The pattern compiled and can be executed.
    Compiled(Regex),
    /// PHP rejects the pattern and `preg_replace_callback()` returns null.
    Invalid,
    /// A real PHP modifier (`A`, `D`, `S`, `X`, `J`, `n`) that eval does not implement.
    UnsupportedModifier,
}

/// Compiles one callback pattern, keeping unmapped modifiers distinct from invalid patterns.
///
/// Unknown modifiers, a missing delimiter, and a PCRE compile failure are
/// `Invalid`. A modifier PHP accepts but this runtime does not implement is
/// `UnsupportedModifier`, which the caller reports as the previous explicit error.
pub(in crate::interpreter) fn eval_preg_callback_pattern(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalPregCallbackPattern, EvalStatus> {
    let pattern = values.string_bytes(pattern)?;
    Ok(compile_preg_callback_pattern(&pattern))
}

/// Classifies one pattern's delimiters and modifiers, then compiles a fully supported pattern.
fn compile_preg_callback_pattern(pattern: &[u8]) -> EvalPregCallbackPattern {
    let (body, modifiers) = match eval_preg_callback_pattern_parts(pattern) {
        Ok(parts) => parts,
        Err(failure) => return failure,
    };
    match Regex::compile(&body, modifiers) {
        Ok(regex) => EvalPregCallbackPattern::Compiled(regex),
        Err(_) => EvalPregCallbackPattern::Invalid,
    }
}

/// Splits a callback pattern, or reports why it cannot be compiled here.
fn eval_preg_callback_pattern_parts(
    pattern: &[u8],
) -> Result<(Vec<u8>, EvalPregModifiers), EvalPregCallbackPattern> {
    if pattern.len() < 2
        || pattern[0].is_ascii_alphanumeric()
        || pattern[0].is_ascii_whitespace()
        || pattern[0] == b'\\'
    {
        return Err(EvalPregCallbackPattern::Invalid);
    }
    let delimiter = pattern[0];
    let closing = eval_preg_closing_delimiter(delimiter);
    let Some(close_index) = eval_preg_find_closing_delimiter(pattern, closing) else {
        return Err(EvalPregCallbackPattern::Invalid);
    };
    let body = eval_preg_unescape_delimiter(&pattern[1..close_index], delimiter, closing);
    let modifiers = eval_preg_callback_modifiers(&pattern[close_index + 1..])?;
    Ok((body, modifiers))
}

/// Parses callback modifiers.
///
/// Implemented modifiers build the flag word. Space, LF, and CR are ignored,
/// matching PHP. Valid PHP modifiers this runtime does not map (`A`, `D`, `S`,
/// `X`, `J`, and PHP 8.4 `n`) are an explicit error. Any other byte, including
/// the removed `e`, is an invalid pattern.
fn eval_preg_callback_modifiers(
    modifiers: &[u8],
) -> Result<EvalPregModifiers, EvalPregCallbackPattern> {
    let mut parsed = EvalPregModifiers::default();
    let mut unsupported = false;
    for modifier in modifiers {
        match *modifier {
            b'i' => parsed.case_insensitive = true,
            b'm' => parsed.multi_line = true,
            b's' => parsed.dot_matches_new_line = true,
            b'U' => parsed.swap_greed = true,
            b'u' => parsed.unicode = true,
            b'x' => parsed.extended = true,
            b' ' | b'\n' | b'\r' => {}
            b'A' | b'D' | b'S' | b'X' | b'J' | b'n' => unsupported = true,
            _ => return Err(EvalPregCallbackPattern::Invalid),
        }
    }
    if unsupported {
        return Err(EvalPregCallbackPattern::UnsupportedModifier);
    }
    Ok(parsed)
}

/// Parses eval-supported PHP regex modifiers.
///
/// Space, LF, and CR are ignored. `x` requests extended mode. Any other byte,
/// including a valid modifier this runtime does not map, is a runtime fatal.
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
            b'x' => parsed.extended = true,
            b' ' | b'\n' | b'\r' => {}
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(parsed)
}
