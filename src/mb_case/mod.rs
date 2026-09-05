//! Purpose:
//! PHP 8.5 UTF-8 `mb_convert_case()` semantics: Unicode full/simple mappings,
//! title-case `Cased`/`Case_Ignorable` state, and Greek final-sigma rules.
//!
//! Called from:
//! - Magician's eval builtin (logic mirrored in the interpreter home file).
//! - AOT runtime table emission under `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Modes `0..=7` match php-src `PHP_UNICODE_CASE_*`.
//! - Full mappings may expand (ß → SS); simple mappings stay 1:1.
//! - Title case follows php-src `php_unicode_convert_case()`: `title_mode` flips
//!   only on non-`Case_Ignorable` code points, and equals `is_cased` of that point.

mod case_ignorable;
mod tables;

pub(crate) use case_ignorable::CASE_IGNORABLE_RANGES;
pub(crate) use tables::{case_tables, FullMap};

/// PHP `MB_CASE_UPPER`.
pub(crate) const MB_CASE_UPPER: i64 = 0;
/// PHP `MB_CASE_LOWER`.
pub(crate) const MB_CASE_LOWER: i64 = 1;
/// PHP `MB_CASE_TITLE`.
pub(crate) const MB_CASE_TITLE: i64 = 2;
/// PHP `MB_CASE_FOLD`.
#[cfg(test)]
const MB_CASE_FOLD: i64 = 3;
/// PHP `MB_CASE_UPPER_SIMPLE`.
#[cfg(test)]
const MB_CASE_UPPER_SIMPLE: i64 = 4;
/// PHP `MB_CASE_LOWER_SIMPLE`.
#[cfg(test)]
const MB_CASE_LOWER_SIMPLE: i64 = 5;
/// PHP `MB_CASE_TITLE_SIMPLE`.
#[cfg(test)]
const MB_CASE_TITLE_SIMPLE: i64 = 6;
/// PHP `MB_CASE_FOLD_SIMPLE`.
#[cfg(test)]
const MB_CASE_FOLD_SIMPLE: i64 = 7;

/// Returns whether `code` is Unicode `Case_Ignorable`.
#[cfg(test)]
fn is_case_ignorable(code: u32) -> bool {
    CASE_IGNORABLE_RANGES
        .binary_search_by(|&(lo, hi)| {
            if code < lo {
                std::cmp::Ordering::Greater
            } else if code > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Returns whether `code` is Unicode `Cased` (lowercase, uppercase, or titlecase).
pub(crate) fn is_cased(code: u32) -> bool {
    char::from_u32(code).is_some_and(|ch| ch.is_lowercase() || ch.is_uppercase())
}

/// Maps one code point with PHP full/simple case conversion.
///
/// The returned slice length is 1..=3. NUL (`U+0000`) is a valid mapped scalar.
#[cfg(test)]
fn map_codepoint(code: u32, mode: i64, title_mode: bool) -> Vec<u32> {
    let Some(ch) = char::from_u32(code) else {
        return vec![code];
    };
    match mode {
        MB_CASE_UPPER => collect_full(ch.to_uppercase(), code),
        MB_CASE_LOWER => collect_full(ch.to_lowercase(), code),
        MB_CASE_FOLD => collect_full(casefold_chars(ch), code),
        MB_CASE_TITLE => {
            if title_mode {
                collect_full(ch.to_lowercase(), code)
            } else {
                collect_full(titlecase_chars(ch), code)
            }
        }
        MB_CASE_UPPER_SIMPLE => vec![simple_or_self(ch.to_uppercase(), code)],
        MB_CASE_LOWER_SIMPLE => vec![simple_or_self(ch.to_lowercase(), code)],
        MB_CASE_FOLD_SIMPLE => vec![simple_or_self(casefold_chars(ch), code)],
        MB_CASE_TITLE_SIMPLE => {
            if title_mode {
                vec![simple_or_self(ch.to_lowercase(), code)]
            } else {
                vec![simple_or_self(titlecase_chars(ch), code)]
            }
        }
        _ => vec![code],
    }
}

/// Converts UTF-8 bytes with PHP 8.5 `mb_convert_case()` semantics.
///
/// Malformed sequences are copied through as a single non-cased, non-ignorable unit.
#[cfg(test)]
fn convert_utf8(bytes: &[u8], mode: i64) -> Vec<u8> {
    let units = decode_utf8_units(bytes);
    apply_case_to_units(&units, mode)
        .into_iter()
        .flat_map(encode_unit)
        .collect()
}

/// Converts 8-bit / binary / 7-bit bytes by treating each byte as `U+00xx`.
#[cfg(test)]
fn convert_latin1_bytes(bytes: &[u8], mode: i64) -> Vec<u8> {
    let units: Vec<DecodedUnit> = bytes
        .iter()
        .map(|&b| DecodedUnit::Scalar(b as u32))
        .collect();
    apply_case_to_units(&units, mode)
        .into_iter()
        .flat_map(|code| {
            if code <= 0xFF {
                vec![code as u8]
            } else {
                let mut buf = [0u8; 4];
                char::from_u32(code)
                    .unwrap_or('\u{FFFD}')
                    .encode_utf8(&mut buf)
                    .as_bytes()
                    .to_vec()
            }
        })
        .collect()
}

/// One decoded input unit: a Unicode scalar or a raw malformed byte slice.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
enum DecodedUnit {
    Scalar(u32),
    Raw(Vec<u8>),
}

/// Returns the Unicode titlecase mapping for `ch`.
///
/// Rust's `char` API has no titlecase iterator. Full titlecase is the uppercase
/// expansion with every character after the first lowercased, except the Unicode
/// digraphs whose titlecase letter is distinct from their uppercase letter.
pub(super) fn titlecase_chars(ch: char) -> Vec<char> {
    if let Some(mapped) = digraph_title(ch) {
        return vec![mapped];
    }
    let upper: Vec<char> = ch.to_uppercase().collect();
    match upper.as_slice() {
        [] => vec![ch],
        [only] => vec![*only],
        [first, rest @ ..] => {
            let mut out = vec![*first];
            for next in rest {
                out.extend(next.to_lowercase());
            }
            out.truncate(3);
            out
        }
    }
}

/// Returns the Unicode full case-fold mapping for `ch`.
///
/// Full case fold matches lowercase except for expansions such as `ß` → `ss`
/// and the alphabetic ligatures, which PHP's `MB_CASE_FOLD` applies.
pub(super) fn casefold_chars(ch: char) -> Vec<char> {
    match ch {
        'ß' => vec!['s', 's'],
        'ﬀ' => vec!['f', 'f'],
        'ﬁ' => vec!['f', 'i'],
        'ﬂ' => vec!['f', 'l'],
        'ﬃ' => vec!['f', 'f', 'i'],
        'ﬄ' => vec!['f', 'f', 'l'],
        'ﬅ' | 'ﬆ' => vec!['s', 't'],
        _ => ch.to_lowercase().collect(),
    }
}

/// Returns the Unicode titlecase letter for DZ/LJ/NJ digraphs.
fn digraph_title(ch: char) -> Option<char> {
    let mapped = match ch as u32 {
        0x01C4 | 0x01C5 | 0x01C6 => 0x01C5,
        0x01C7 | 0x01C8 | 0x01C9 => 0x01C8,
        0x01CA | 0x01CB | 0x01CC => 0x01CB,
        0x01F1 | 0x01F2 | 0x01F3 => 0x01F2,
        _ => return None,
    };
    char::from_u32(mapped)
}

/// Collects a full Unicode case mapping, falling back to `original` if the iterator is empty.
pub(crate) fn collect_full<I>(iter: I, original: u32) -> Vec<u32>
where
    I: IntoIterator<Item = char>,
{
    let mapped: Vec<u32> = iter.into_iter().take(3).map(|ch| ch as u32).collect();
    if mapped.is_empty() {
        vec![original]
    } else {
        mapped
    }
}

/// Returns the single-code-point mapping, or `original` when the full map expands.
pub(crate) fn simple_or_self<I>(iter: I, original: u32) -> u32
where
    I: IntoIterator<Item = char>,
{
    let mapped: Vec<char> = iter.into_iter().collect();
    match mapped.as_slice() {
        [ch] => *ch as u32,
        _ => original,
    }
}

/// Decodes UTF-8 into scalars, grouping each malformed sequence as one raw unit.
#[cfg(test)]
fn decode_utf8_units(bytes: &[u8]) -> Vec<DecodedUnit> {
    let mut units = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                units.extend(valid.chars().map(|ch| DecodedUnit::Scalar(ch as u32)));
                break;
            }
            Err(error) => {
                let valid_len = error.valid_up_to();
                if valid_len > 0 {
                    let valid = std::str::from_utf8(&bytes[offset..offset + valid_len])
                        .expect("from_utf8 valid prefix");
                    units.extend(valid.chars().map(|ch| DecodedUnit::Scalar(ch as u32)));
                }
                match error.error_len() {
                    Some(invalid_len) => {
                        units.push(DecodedUnit::Raw(
                            bytes[offset + valid_len..offset + valid_len + invalid_len].to_vec(),
                        ));
                        offset += valid_len + invalid_len;
                    }
                    None => {
                        units.push(DecodedUnit::Raw(bytes[offset + valid_len..].to_vec()));
                        break;
                    }
                }
            }
        }
    }
    units
}

/// Applies PHP case conversion, including title-case state and final-sigma rules.
#[cfg(test)]
fn apply_case_to_units(units: &[DecodedUnit], mode: i64) -> Vec<u32> {
    let mut out = Vec::new();
    let mut title_mode = false;
    let full_lower_or_title = mode == MB_CASE_LOWER || mode == MB_CASE_TITLE;
    for (index, unit) in units.iter().enumerate() {
        match unit {
            DecodedUnit::Raw(raw) => {
                out.extend(raw.iter().map(|&b| b as u32));
                title_mode = false;
            }
            DecodedUnit::Scalar(code) => {
                if full_lower_or_title
                    && *code == 0x03A3
                    && (mode != MB_CASE_TITLE || title_mode)
                    && should_use_final_sigma(units, index)
                {
                    out.push(0x03C2);
                } else {
                    out.extend(map_codepoint(*code, mode, title_mode));
                }
                if matches!(mode, MB_CASE_TITLE | MB_CASE_TITLE_SIMPLE) && !is_case_ignorable(*code)
                {
                    title_mode = is_cased(*code);
                }
            }
        }
    }
    out
}

/// Implements PHP 8.3+ final-sigma: last cased letter in a word, ignoring Case_Ignorable.
#[cfg(test)]
fn should_use_final_sigma(units: &[DecodedUnit], index: usize) -> bool {
    let mut saw_cased_before = false;
    for unit in units[..index].iter().rev() {
        match unit {
            DecodedUnit::Raw(_) => break,
            DecodedUnit::Scalar(code) => {
                if is_case_ignorable(*code) {
                    continue;
                }
                saw_cased_before = is_cased(*code);
                break;
            }
        }
    }
    if !saw_cased_before {
        return false;
    }
    for unit in units[index + 1..].iter() {
        match unit {
            DecodedUnit::Raw(_) => return true,
            DecodedUnit::Scalar(code) => {
                if is_case_ignorable(*code) {
                    continue;
                }
                return !is_cased(*code);
            }
        }
    }
    true
}

/// Encodes one output code point as UTF-8 bytes, or as a raw byte when it came from malformation.
#[cfg(test)]
fn encode_unit(code: u32) -> Vec<u8> {
    if let Some(ch) = char::from_u32(code) {
        let mut buf = [0u8; 4];
        ch.encode_utf8(&mut buf).as_bytes().to_vec()
    } else if code <= 0xFF {
        vec![code as u8]
    } else {
        vec![b'?']
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies ASCII title case, the Termwind capitalize path, and ß expansion.
    #[test]
    fn utf8_title_and_upper_match_php_examples() {
        assert_eq!(
            convert_utf8(b"hello world", MB_CASE_TITLE),
            b"Hello World"
        );
        assert_eq!(
            convert_utf8("mary had a Little lamb".as_bytes(), MB_CASE_TITLE),
            b"Mary Had A Little Lamb"
        );
        assert_eq!(convert_utf8("don't stop".as_bytes(), MB_CASE_TITLE), b"Don't Stop");
        assert_eq!(convert_utf8("straße".as_bytes(), MB_CASE_UPPER), "STRASSE".as_bytes());
        assert_eq!(convert_utf8("straße".as_bytes(), MB_CASE_UPPER_SIMPLE), "STRAßE".as_bytes());
        assert_eq!(convert_utf8("ß".as_bytes(), MB_CASE_TITLE), b"Ss");
        assert_eq!(convert_utf8("ǆungla".as_bytes(), MB_CASE_TITLE), "ǅungla".as_bytes());
        assert_eq!(convert_utf8("Straße".as_bytes(), MB_CASE_LOWER), "straße".as_bytes());
        assert_eq!(convert_utf8("Straße".as_bytes(), MB_CASE_FOLD), "strasse".as_bytes());
        assert_eq!(convert_latin1_bytes(&[0xE9], MB_CASE_UPPER), &[0xC9]);
    }

    /// Verifies Case_Ignorable includes apostrophe and excludes quotation marks.
    #[test]
    fn case_ignorable_covers_apostrophe_not_quote() {
        assert!(is_case_ignorable(0x27));
        assert!(!is_case_ignorable(0x22));
        assert!(is_cased(b'A' as u32));
        assert!(!is_cased(b' ' as u32));
    }
}
