//! Purpose:
//! Ports php-src `ext/filter/sanitizing_filters.c` string transformations.
//!
//! Called from:
//! - `crate::elephc_filter_apply` for the sanitizing filter ids.
//!
//! Key details:
//! - All maps are byte-oriented, so high bytes, NUL and invalid UTF-8 survive exactly as PHP's
//!   binary-safe filter implementation handles them.
//! - `FILTER_SANITIZE_FULL_SPECIAL_CHARS` and `FILTER_SANITIZE_ADD_SLASHES` are handled by the
//!   PHP layer because they directly delegate to `htmlspecialchars()` and `addslashes()`.

use crate::strip_tags::strip;
use crate::Outcome;

/// FILTER_FLAG_STRIP_LOW.
const STRIP_LOW: i64 = 0x0004;
/// FILTER_FLAG_STRIP_HIGH.
const STRIP_HIGH: i64 = 0x0008;
/// FILTER_FLAG_ENCODE_LOW.
const ENCODE_LOW: i64 = 0x0010;
/// FILTER_FLAG_ENCODE_HIGH.
const ENCODE_HIGH: i64 = 0x0020;
/// FILTER_FLAG_ENCODE_AMP.
const ENCODE_AMP: i64 = 0x0040;
/// FILTER_FLAG_NO_ENCODE_QUOTES.
const NO_ENCODE_QUOTES: i64 = 0x0080;
/// FILTER_FLAG_EMPTY_STRING_NULL.
const EMPTY_STRING_NULL: i64 = 0x0100;
/// FILTER_FLAG_STRIP_BACKTICK.
const STRIP_BACKTICK: i64 = 0x0200;
/// FILTER_FLAG_ALLOW_FRACTION.
const ALLOW_FRACTION: i64 = 0x1000;
/// FILTER_FLAG_ALLOW_THOUSAND.
const ALLOW_THOUSAND: i64 = 0x2000;
/// FILTER_FLAG_ALLOW_SCIENTIFIC.
const ALLOW_SCIENTIFIC: i64 = 0x4000;

/// FILTER_SANITIZE_STRING and FILTER_SANITIZE_STRIPPED.
const SANITIZE_STRING: i64 = 0x0201;
/// FILTER_SANITIZE_ENCODED.
const SANITIZE_ENCODED: i64 = 0x0202;
/// FILTER_SANITIZE_SPECIAL_CHARS.
const SANITIZE_SPECIAL: i64 = 0x0203;
/// FILTER_DEFAULT / FILTER_UNSAFE_RAW.
const UNSAFE_RAW: i64 = 0x0204;
/// FILTER_SANITIZE_EMAIL.
const SANITIZE_EMAIL: i64 = 0x0205;
/// FILTER_SANITIZE_URL.
const SANITIZE_URL: i64 = 0x0206;
/// FILTER_SANITIZE_NUMBER_INT.
const NUMBER_INT: i64 = 0x0207;
/// FILTER_SANITIZE_NUMBER_FLOAT.
const NUMBER_FLOAT: i64 = 0x0208;

/// Applies one php_filter_* sanitizer and returns its string or successful-null outcome.
pub(crate) fn apply(filter: i64, flags: i64, value: &[u8]) -> Outcome {
    match filter {
        SANITIZE_STRING => {
            let stripped = strip_value(value, flags);
            let mut encoded = vec![false; 256];
            if flags & NO_ENCODE_QUOTES == 0 {
                encoded[b'\'' as usize] = true;
                encoded[b'"' as usize] = true;
            }
            if flags & ENCODE_AMP != 0 { encoded[b'&' as usize] = true; }
            if flags & ENCODE_LOW != 0 { encoded[..32].fill(true); }
            if flags & ENCODE_HIGH != 0 { encoded[127..].fill(true); }
            let result = strip(&encode_html(&stripped, &encoded));
            if result.is_empty() && flags & EMPTY_STRING_NULL != 0 { Outcome::Null } else { Outcome::Str(result) }
        }
        SANITIZE_ENCODED => Outcome::Str(encode_url(&strip_value(value, flags))),
        SANITIZE_SPECIAL => {
            let stripped = strip_value(value, flags);
            let mut encoded = vec![false; 256];
            for byte in [b'\'', b'"', b'<', b'>', b'&', 0] {
                encoded[byte as usize] = true;
            }
            encoded[..32].fill(true);
            if flags & ENCODE_HIGH != 0 { encoded[127..].fill(true); }
            Outcome::Str(encode_html(&stripped, &encoded))
        }
        UNSAFE_RAW => {
            if flags != 0 && !value.is_empty() {
                let stripped = strip_value(value, flags);
                let mut encoded = vec![false; 256];
                if flags & ENCODE_AMP != 0 { encoded[b'&' as usize] = true; }
                if flags & ENCODE_LOW != 0 { encoded[..32].fill(true); }
                if flags & ENCODE_HIGH != 0 { encoded[127..].fill(true); }
                Outcome::Str(encode_html(&stripped, &encoded))
            } else if flags & EMPTY_STRING_NULL != 0 && value.is_empty() {
                Outcome::Null
            } else {
                Outcome::Str(value.to_vec())
            }
        }
        SANITIZE_EMAIL => Outcome::Str(map_apply(
            value,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&'*+-=?^_`{|}~@.[]",
        )),
        SANITIZE_URL => Outcome::Str(map_apply(
            value,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789$-_.+!*'(),{}|\\^~[]`<>#%\";/?:@&=",
        )),
        NUMBER_INT => Outcome::Str(map_apply(value, b"+-0123456789")),
        NUMBER_FLOAT => {
            let mut allowed = b"+-0123456789".to_vec();
            if flags & ALLOW_FRACTION != 0 { allowed.push(b'.'); }
            if flags & ALLOW_THOUSAND != 0 { allowed.push(b','); }
            if flags & ALLOW_SCIENTIFIC != 0 { allowed.extend_from_slice(b"eE"); }
            Outcome::Str(map_apply(value, &allowed))
        }
        _ => Outcome::Str(value.to_vec()),
    }
}

/// Applies php_filter_strip's low/high/backtick removal.
fn strip_value(value: &[u8], flags: i64) -> Vec<u8> {
    if flags & (STRIP_LOW | STRIP_HIGH | STRIP_BACKTICK) == 0 { return value.to_vec(); }
    value
        .iter()
        .copied()
        .filter(|byte| {
            !(*byte >= 127 && flags & STRIP_HIGH != 0)
                && !(*byte < 32 && flags & STRIP_LOW != 0)
                && !(*byte == b'`' && flags & STRIP_BACKTICK != 0)
        })
        .collect()
}

/// Encodes every byte flagged in `encoded` as `&#<decimal>;`.
fn encode_html(value: &[u8], encoded: &[bool]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for &byte in value {
        if encoded[byte as usize] {
            out.extend_from_slice(b"&#");
            out.extend_from_slice(byte.to_string().as_bytes());
            out.push(b';');
        } else {
            out.push(byte);
        }
    }
    out
}

/// Percent-encodes every byte outside `DEFAULT_URL_ENCODE`.
fn encode_url(value: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(value.len());
    for &byte in value {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_') {
            out.push(byte);
        } else {
            out.push(b'%');
            out.push(HEX[(byte >> 4) as usize]);
            out.push(HEX[(byte & 0x0f) as usize]);
        }
    }
    out
}

/// Retains only bytes present in `allowed`.
fn map_apply(value: &[u8], allowed: &[u8]) -> Vec<u8> {
    let mut keep = [false; 256];
    for &byte in allowed { keep[byte as usize] = true; }
    value.iter().copied().filter(|byte| keep[*byte as usize]).collect()
}
