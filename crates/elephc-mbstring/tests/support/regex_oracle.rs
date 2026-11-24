//! Purpose:
//! Documents security divergences between the captured PHP mbregex oracle and elephc.
//!
//! Called from:
//! - `tests/regex_request.rs` and `tests/invoke/regex_request.rs`.
//!
//! Key details:
//! - PHP 8.5.13 casts numeric `\k<...>` group numbers through `(int) strtoul`; elephc
//!   keeps the escape literal when the number exceeds `i32::MAX`
//!   (PHP-PrettyHorriblePatches #34).
//! - PHP 8.5.13 also exposes its string terminator for an unterminated `\k`/`\k<...`
//!   form; elephc copies only the bytes that belong to the replacement (#17).
//! - The fixture stays a faithful PHP capture. The fixed behavior is covered by the
//!   dedicated codegen and unit regressions, and these cases are skipped here.

use serde_json::Value;

/// Decodes the oracle's even-length hexadecimal byte field.
fn hex_bytes(value: &str) -> Option<Vec<u8>> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

/// Returns true when any step hits a documented PHP oracle divergence.
///
/// This covers numeric `\k` groups above `i32::MAX` and unterminated `\k` forms whose
/// PHP output includes its string terminator. The skip counts pinned by
/// `regex_replace_matches_php` belong to the current fixture: a fresh PHP capture
/// must re-derive them instead of carrying the old numbers forward.
pub(crate) fn has_security_divergence(case: &Value) -> bool {
    case["steps"]
        .as_array()
        .is_some_and(|steps| steps.iter().any(step_has_security_divergence))
}

/// Returns true when one replacement step hits a documented PHP oracle divergence.
///
/// Per-step classification keeps the remaining steps of the same case compared
/// against PHP instead of dropping the whole case.
pub(crate) fn step_has_security_divergence(step: &Value) -> bool {
    let Some(raw) = step["replacement"].as_str().and_then(hex_bytes) else {
        return false;
    };
    let mut cursor = 0;
    while cursor + 2 <= raw.len() {
        let Some(offset) = raw[cursor..]
            .windows(2)
            .position(|window| window == b"\\k")
        else {
            break;
        };
        let group = cursor + offset + 2;
        if group == raw.len() {
            return true;
        }
        let delimiter = match raw[group] {
            b'<' => Some(b'>'),
            b'\'' => Some(b'\''),
            _ => None,
        };
        let Some(delimiter) = delimiter else {
            cursor = group;
            continue;
        };
        let mut end = group + 1;
        while end < raw.len() && raw[end] != delimiter {
            end += 1;
        }
        if end == raw.len() {
            return true;
        }
        let name = &raw[group + 1..end];
        if !name.is_empty()
            && !(name.len() > 1 && name[0] == b'0')
            && name.iter().all(u8::is_ascii_digit)
        {
            let number = name.iter().fold(0u64, |value, byte| {
                value.saturating_mul(10).saturating_add(u64::from(byte - b'0'))
            });
            if number > i32::MAX as u64 {
                return true;
            }
        }
        cursor = end + 1;
    }
    false
}
