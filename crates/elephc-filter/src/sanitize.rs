//! Purpose:
//! Ports php-src `ext/filter/sanitizing_filters.c` string transformations.
//!
//! Called from:
//! - `crate::elephc_filter_apply` for the sanitizing filter ids.
//!
//! Key details:
//! - All maps are byte-oriented, so high bytes, NUL and invalid UTF-8 survive exactly as PHP's
//!   binary-safe filter implementation handles them.
//! - `FILTER_SANITIZE_FULL_SPECIAL_CHARS` uses the HTML 4.01 entity table and the configured
//!   charset; `FILTER_SANITIZE_ADD_SLASHES` stays in the PHP layer because it delegates to
//!   `addslashes()`.

use crate::entities;
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

/// HTML charset families the full-special sanitizer converts to named entities.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HtmlCharset {
    Utf8,
    Latin1,
    Windows1252,
    Iso885915,
}

/// Resolves `default_charset` to the charset family used by the entity encoder.
fn html_charset(name: &str) -> HtmlCharset {
    match name.trim().to_ascii_lowercase().as_str() {
        "iso-8859-1" | "iso8859-1" | "latin1" => HtmlCharset::Latin1,
        "windows-1252" | "cp1252" => HtmlCharset::Windows1252,
        "iso-8859-15" | "iso8859-15" | "latin9" => HtmlCharset::Iso885915,
        _ => HtmlCharset::Utf8,
    }
}

/// Applies php-src's `FILTER_SANITIZE_FULL_SPECIAL_CHARS` entity encoding.
///
/// The input is decoded with the configured charset, HTML metacharacters are escaped,
/// existing HTML 4.01 entities are preserved, and non-ASCII code points are rendered
/// with their named entity when one exists. Invalid UTF-8 yields PHP's empty string.
pub(crate) fn full_special_chars(flags: i64, value: &[u8], charset: &str) -> Outcome {
    let no_quotes = flags & NO_ENCODE_QUOTES != 0;
    match html_charset(charset) {
        HtmlCharset::Utf8 => match std::str::from_utf8(value) {
            Ok(text) => Outcome::Str(encode_utf8_entities(text, no_quotes)),
            Err(_) => Outcome::Str(Vec::new()),
        },
        family => Outcome::Str(encode_single_byte_entities(value, no_quotes, family)),
    }
}

/// Encodes one UTF-8 string with PHP's HTML 4.01 entity rules.
fn encode_utf8_entities(text: &str, no_quotes: bool) -> Vec<u8> {
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < text.len() {
        let ch = text[index..]
            .chars()
            .next()
            .expect("valid UTF-8 boundary");
        if ch == '&' {
            if let Some(length) = entity_length(&text[index..]) {
                out.push_str(&text[index..index + length]);
                index += length;
                continue;
            }
            out.push_str("&amp;");
        } else if ch == '<' {
            out.push_str("&lt;");
        } else if ch == '>' {
            out.push_str("&gt;");
        } else if ch == '"' && !no_quotes {
            out.push_str("&quot;");
        } else if ch == '\'' && !no_quotes {
            out.push_str("&#039;");
        } else if (ch as u32) >= 0x80 {
            if let Some(entity) = entities::entity_for_code_point(ch as u32) {
                out.push_str(entity);
            } else {
                out.push(ch);
            }
        } else {
            out.push(ch);
        }
        index += ch.len_utf8();
    }
    out.into_bytes()
}

/// Encodes one single-byte string with PHP's HTML 4.01 entity rules.
fn encode_single_byte_entities(value: &[u8], no_quotes: bool, charset: HtmlCharset) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let mut index = 0usize;
    while index < value.len() {
        let byte = value[index];
        if byte == b'&' {
            if let Some(length) = byte_entity_length(&value[index..]) {
                out.extend_from_slice(&value[index..index + length]);
                index += length;
                continue;
            }
            out.extend_from_slice(b"&amp;");
        } else if byte == b'<' {
            out.extend_from_slice(b"&lt;");
        } else if byte == b'>' {
            out.extend_from_slice(b"&gt;");
        } else if byte == b'"' && !no_quotes {
            out.extend_from_slice(b"&quot;");
        } else if byte == b'\'' && !no_quotes {
            out.extend_from_slice(b"&#039;");
        } else if byte >= 0x80 {
            let code_point = single_byte_code_point(charset, byte);
            if let Some(entity) = entities::entity_for_code_point(code_point) {
                out.extend_from_slice(entity.as_bytes());
            } else {
                out.push(byte);
            }
        } else {
            out.push(byte);
        }
        index += 1;
    }
    out
}

/// Upper bound on an unterminated `&...` scan. A candidate that starts with a legal entity
/// character is still bounded so a pathological input cannot rescan the whole suffix per `&`.
const MAX_ENTITY_BYTES: usize = 4096;

/// Returns whether the byte after `&` can start a named or numeric entity.
///
/// Rejecting a bare `&` immediately is what keeps a run of ampersands linear: without it,
/// every `&` would search the remaining string for a `;`.
fn starts_entity(candidate: u8) -> bool {
    candidate == b'#' || candidate.is_ascii_alphanumeric()
}

/// Returns the byte length of a valid HTML entity at the start of `rest`.
fn entity_length(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.len() < 3 || !starts_entity(bytes[1]) { return None; }
    let limit = rest.len().min(MAX_ENTITY_BYTES);
    let end = bytes[..limit].iter().position(|byte| *byte == b';')?;
    let entity = &rest[..=end];
    let name = entity.strip_prefix('&')?.strip_suffix(';')?;
    (entities::is_named_entity(name) || entities::is_numeric_entity(entity)).then_some(end + 1)
}

/// Returns the byte length of a valid ASCII HTML entity at the start of `rest`.
fn byte_entity_length(rest: &[u8]) -> Option<usize> {
    if rest.len() < 3 || !starts_entity(rest[1]) { return None; }
    let limit = rest.len().min(MAX_ENTITY_BYTES);
    let end = rest[..limit].iter().position(|byte| *byte == b';')?;
    let entity = std::str::from_utf8(&rest[..=end]).ok()?;
    let name = entity.strip_prefix('&')?.strip_suffix(';')?;
    (entities::is_named_entity(name) || entities::is_numeric_entity(entity)).then_some(end + 1)
}

/// Maps one single-byte character to its Unicode code point for the selected charset.
fn single_byte_code_point(charset: HtmlCharset, byte: u8) -> u32 {
    match charset {
        HtmlCharset::Latin1 | HtmlCharset::Utf8 => u32::from(byte),
        HtmlCharset::Windows1252 => match byte {
            0x80 => 0x20AC,
            0x82 => 0x201A,
            0x83 => 0x0192,
            0x84 => 0x201E,
            0x85 => 0x2026,
            0x86 => 0x2020,
            0x87 => 0x2021,
            0x88 => 0x02C6,
            0x89 => 0x2030,
            0x8A => 0x0160,
            0x8B => 0x2039,
            0x8C => 0x0152,
            0x8E => 0x017D,
            0x91 => 0x2018,
            0x92 => 0x2019,
            0x93 => 0x201C,
            0x94 => 0x201D,
            0x95 => 0x2022,
            0x96 => 0x2013,
            0x97 => 0x2014,
            0x98 => 0x02DC,
            0x99 => 0x2122,
            0x9A => 0x0161,
            0x9B => 0x203A,
            0x9C => 0x0153,
            0x9E => 0x017E,
            0x9F => 0x0178,
            _ => u32::from(byte),
        },
        HtmlCharset::Iso885915 => match byte {
            0xA4 => 0x20AC,
            0xA6 => 0x0160,
            0xA8 => 0x0161,
            0xB4 => 0x017D,
            0xB8 => 0x017E,
            0xBC => 0x0152,
            0xBD => 0x0153,
            0xBE => 0x0178,
            _ => u32::from(byte),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches PHP's UTF-8 entity output, existing-entity preservation and invalid input.
    #[test]
    fn full_special_chars_matches_php_for_utf8() {
        assert_eq!(
            full_special_chars(0, b"caf\xC3\xA9 <b>", "UTF-8"),
            Outcome::Str(b"caf&eacute; &lt;b&gt;".to_vec())
        );
        assert_eq!(
            full_special_chars(0, b"caf&eacute; <b>", "UTF-8"),
            Outcome::Str(b"caf&eacute; &lt;b&gt;".to_vec())
        );
        assert_eq!(
            full_special_chars(0, b"\"'", "UTF-8"),
            Outcome::Str(b"&quot;&#039;".to_vec())
        );
        assert_eq!(
            full_special_chars(NO_ENCODE_QUOTES, b"\"'", "UTF-8"),
            Outcome::Str(b"\"'".to_vec())
        );
        assert_eq!(
            full_special_chars(0, b"\xC3", "UTF-8"),
            Outcome::Str(Vec::new())
        );
    }

    /// Matches PHP's entity output for the supported single-byte charsets.
    #[test]
    fn full_special_chars_matches_php_for_single_byte_charsets() {
        assert_eq!(
            full_special_chars(0, b"caf\xE9 <b>", "ISO-8859-1"),
            Outcome::Str(b"caf&eacute; &lt;b&gt;".to_vec())
        );
        assert_eq!(
            full_special_chars(0, b"\x80", "windows-1252"),
            Outcome::Str(b"&euro;".to_vec())
        );
        assert_eq!(
            full_special_chars(0, b"\xA4", "ISO-8859-15"),
            Outcome::Str(b"&euro;".to_vec())
        );
    }
}
