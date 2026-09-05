//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `mb_strimwidth()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - The eval signature matches PHP: string, start, width, optional trim marker, optional encoding.
//! - UTF-8 (default/`null`/`UTF-8`/`UTF8`) trims by East Asian display width from PHP 8.5's
//!   `eaw_table.h`. `8bit`/`binary`/`7bit` treat every byte as width 1.
//! - Unknown encodings and out-of-range `$start`/`$width` raise catchable `ValueError`.

use super::super::super::*;

eval_builtin! {
    contract: "mb_strimwidth",
    area: String,
    direct: MbStrimwidth,
    values: MbStrimwidth,
}

/// Inclusive Unicode ranges that PHP 8.5 treats as display width 2.
const EAW_RANGES: &[(u32, u32)] = &[
    (0x1100, 0x115f),
    (0x231a, 0x231b),
    (0x2329, 0x232a),
    (0x23e9, 0x23ec),
    (0x23f0, 0x23f0),
    (0x23f3, 0x23f3),
    (0x25fd, 0x25fe),
    (0x2614, 0x2615),
    (0x2630, 0x2637),
    (0x2648, 0x2653),
    (0x267f, 0x267f),
    (0x268a, 0x268f),
    (0x2693, 0x2693),
    (0x26a1, 0x26a1),
    (0x26aa, 0x26ab),
    (0x26bd, 0x26be),
    (0x26c4, 0x26c5),
    (0x26ce, 0x26ce),
    (0x26d4, 0x26d4),
    (0x26ea, 0x26ea),
    (0x26f2, 0x26f3),
    (0x26f5, 0x26f5),
    (0x26fa, 0x26fa),
    (0x26fd, 0x26fd),
    (0x2705, 0x2705),
    (0x270a, 0x270b),
    (0x2728, 0x2728),
    (0x274c, 0x274c),
    (0x274e, 0x274e),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2795, 0x2797),
    (0x27b0, 0x27b0),
    (0x27bf, 0x27bf),
    (0x2b1b, 0x2b1c),
    (0x2b50, 0x2b50),
    (0x2b55, 0x2b55),
    (0x2e80, 0x2e99),
    (0x2e9b, 0x2ef3),
    (0x2f00, 0x2fd5),
    (0x2ff0, 0x303e),
    (0x3041, 0x3096),
    (0x3099, 0x30ff),
    (0x3105, 0x312f),
    (0x3131, 0x318e),
    (0x3190, 0x31e5),
    (0x31ef, 0x321e),
    (0x3220, 0x3247),
    (0x3250, 0xa48c),
    (0xa490, 0xa4c6),
    (0xa960, 0xa97c),
    (0xac00, 0xd7a3),
    (0xf900, 0xfaff),
    (0xfe10, 0xfe19),
    (0xfe30, 0xfe52),
    (0xfe54, 0xfe66),
    (0xfe68, 0xfe6b),
    (0xff01, 0xff60),
    (0xffe0, 0xffe6),
    (0x16fe0, 0x16fe4),
    (0x16ff0, 0x16ff6),
    (0x17000, 0x18cd5),
    (0x18cff, 0x18d1e),
    (0x18d80, 0x18df2),
    (0x1aff0, 0x1aff3),
    (0x1aff5, 0x1affb),
    (0x1affd, 0x1affe),
    (0x1b000, 0x1b122),
    (0x1b132, 0x1b132),
    (0x1b150, 0x1b152),
    (0x1b155, 0x1b155),
    (0x1b164, 0x1b167),
    (0x1b170, 0x1b2fb),
    (0x1d300, 0x1d356),
    (0x1d360, 0x1d376),
    (0x1f004, 0x1f004),
    (0x1f0cf, 0x1f0cf),
    (0x1f18e, 0x1f18e),
    (0x1f191, 0x1f19a),
    (0x1f200, 0x1f202),
    (0x1f210, 0x1f23b),
    (0x1f240, 0x1f248),
    (0x1f250, 0x1f251),
    (0x1f260, 0x1f265),
    (0x1f300, 0x1f320),
    (0x1f32d, 0x1f335),
    (0x1f337, 0x1f37c),
    (0x1f37e, 0x1f393),
    (0x1f3a0, 0x1f3ca),
    (0x1f3cf, 0x1f3d3),
    (0x1f3e0, 0x1f3f0),
    (0x1f3f4, 0x1f3f4),
    (0x1f3f8, 0x1f43e),
    (0x1f440, 0x1f440),
    (0x1f442, 0x1f4fc),
    (0x1f4ff, 0x1f53d),
    (0x1f54b, 0x1f54e),
    (0x1f550, 0x1f567),
    (0x1f57a, 0x1f57a),
    (0x1f595, 0x1f596),
    (0x1f5a4, 0x1f5a4),
    (0x1f5fb, 0x1f64f),
    (0x1f680, 0x1f6c5),
    (0x1f6cc, 0x1f6cc),
    (0x1f6d0, 0x1f6d2),
    (0x1f6d5, 0x1f6d8),
    (0x1f6dc, 0x1f6df),
    (0x1f6eb, 0x1f6ec),
    (0x1f6f4, 0x1f6fc),
    (0x1f7e0, 0x1f7eb),
    (0x1f7f0, 0x1f7f0),
    (0x1f90c, 0x1f93a),
    (0x1f93c, 0x1f945),
    (0x1f947, 0x1f9ff),
    (0x1fa70, 0x1fa7c),
    (0x1fa80, 0x1fa8a),
    (0x1fa8e, 0x1fac6),
    (0x1fac8, 0x1fac8),
    (0x1facd, 0x1fadc),
    (0x1fadf, 0x1faea),
    (0x1faef, 0x1faf8),
    (0x20000, 0x2fffd),
    (0x30000, 0x3fffd),
];

/// Evaluates direct `mb_strimwidth()` calls while preserving PHP source-order evaluation.
pub(in crate::interpreter) fn eval_builtin_mb_strimwidth(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, start, width] => {
            let value = eval_expr(value, context, scope, values)?;
            let start = eval_expr(start, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            eval_mb_strimwidth_result(value, start, width, None, None, context, values)
        }
        [value, start, width, trim_marker] => {
            let value = eval_expr(value, context, scope, values)?;
            let start = eval_expr(start, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let trim_marker = eval_expr(trim_marker, context, scope, values)?;
            eval_mb_strimwidth_result(
                value,
                start,
                width,
                Some(trim_marker),
                None,
                context,
                values,
            )
        }
        [value, start, width, trim_marker, encoding] => {
            let value = eval_expr(value, context, scope, values)?;
            let start = eval_expr(start, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let trim_marker = eval_expr(trim_marker, context, scope, values)?;
            let encoding = eval_expr(encoding, context, scope, values)?;
            eval_mb_strimwidth_result(
                value,
                start,
                width,
                Some(trim_marker),
                Some(encoding),
                context,
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Trims one materialized eval string to a PHP display width with an optional marker.
pub(in crate::interpreter) fn eval_mb_strimwidth_result(
    value: RuntimeCellHandle,
    start: RuntimeCellHandle,
    width: RuntimeCellHandle,
    trim_marker: Option<RuntimeCellHandle>,
    encoding: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let start = eval_int_value(start, values)?;
    let width = eval_int_value(width, values)?;
    let marker = match trim_marker {
        Some(trim_marker) => values.string_bytes(trim_marker)?,
        None => Vec::new(),
    };
    let encoding = match encoding {
        Some(encoding) if !values.is_null(encoding)? => Some(values.string_bytes(encoding)?),
        _ => None,
    };
    let byte_width = match encoding.as_deref() {
        None => false,
        Some(encoding) if is_utf8_encoding(encoding) => false,
        Some(encoding) if is_byte_encoding(encoding) => true,
        Some(encoding) => return eval_mb_strimwidth_encoding_error(encoding, context, values),
    };

    match trim_to_width(&bytes, start, width, &marker, byte_width) {
        Ok(trimmed) => values.string_bytes_value(&trimmed),
        Err(MbStrimwidthError::StartOutOfRange) => eval_throw_builtin_value_error(
            "mb_strimwidth(): Argument #2 ($start) is out of range",
            context,
            values,
        ),
        Err(MbStrimwidthError::WidthOutOfRange) => eval_throw_builtin_value_error(
            "mb_strimwidth(): Argument #3 ($width) is out of range",
            context,
            values,
        ),
    }
}

/// PHP-compatible encoding aliases that use the UTF-8 display-width scanner.
fn is_utf8_encoding(encoding: &[u8]) -> bool {
    encoding.eq_ignore_ascii_case(b"UTF-8") || encoding.eq_ignore_ascii_case(b"UTF8")
}

/// PHP byte-count aliases that treat every byte as one width-1 character.
fn is_byte_encoding(encoding: &[u8]) -> bool {
    encoding.eq_ignore_ascii_case(b"8bit")
        || encoding.eq_ignore_ascii_case(b"binary")
        || encoding.eq_ignore_ascii_case(b"7bit")
}

/// Catchable `ValueError` raised when the encoding name is not a supported alias.
fn eval_mb_strimwidth_encoding_error<T>(
    encoding: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let encoding = String::from_utf8_lossy(encoding);
    let message = format!(
        "mb_strimwidth(): Argument #5 ($encoding) must be a valid encoding, \"{}\" given",
        encoding
    );
    eval_throw_builtin_value_error(&message, context, values)
}

/// Failures that PHP 8.5 reports as argument `ValueError`s for `mb_strimwidth()`.
enum MbStrimwidthError {
    StartOutOfRange,
    WidthOutOfRange,
}

/// Implements PHP 8.5 `mb_trim_string` plus start/negative-width resolution.
fn trim_to_width(
    bytes: &[u8],
    mut start: i64,
    mut width: i64,
    marker: &[u8],
    byte_width: bool,
) -> Result<Vec<u8>, MbStrimwidthError> {
    let char_count = count_chars(bytes, byte_width);
    if start != 0 {
        if start < 0 {
            start += char_count;
        }
        if start < 0 || start > char_count {
            return Err(MbStrimwidthError::StartOutOfRange);
        }
    }
    let start = usize::try_from(start).unwrap_or(0);

    if width < 0 {
        let total_width = i64::try_from(string_width(bytes, byte_width)).unwrap_or(i64::MAX);
        width += total_width;
        if start > 0 {
            let prefix_end = skip_chars(bytes, start, byte_width);
            width -= i64::try_from(string_width(&bytes[..prefix_end], byte_width)).unwrap_or(0);
        }
        if width < 0 {
            return Err(MbStrimwidthError::WidthOutOfRange);
        }
    }
    let width = usize::try_from(width).unwrap_or(0);

    let start_byte = skip_chars(bytes, start, byte_width);
    let rest = &bytes[start_byte..];
    if string_width(rest, byte_width) <= width {
        return Ok(rest.to_vec());
    }

    let marker_width = string_width(marker, byte_width);
    if width <= marker_width {
        return Ok(marker.to_vec());
    }

    let take_end = take_display_width(rest, width - marker_width, byte_width);
    let mut out = rest[..take_end].to_vec();
    out.extend_from_slice(marker);
    Ok(out)
}

/// Counts characters using the same UTF-8 substitution boundaries as `mb_strlen()`.
fn count_chars(bytes: &[u8], byte_width: bool) -> i64 {
    if byte_width {
        return i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    }
    let mut offset = 0usize;
    let mut count = 0i64;
    while let Some((next, _)) = next_char(bytes, offset) {
        count += 1;
        offset = next;
    }
    count
}

/// Returns the East Asian display width of a whole string.
fn string_width(bytes: &[u8], byte_width: bool) -> usize {
    if byte_width {
        return bytes.len();
    }
    let mut offset = 0usize;
    let mut width = 0usize;
    while let Some((next, codepoint)) = next_char(bytes, offset) {
        width += character_width(codepoint);
        offset = next;
    }
    width
}

/// Returns the byte offset after skipping `count` characters from the start of `bytes`.
fn skip_chars(bytes: &[u8], count: usize, byte_width: bool) -> usize {
    if byte_width {
        return count.min(bytes.len());
    }
    let mut offset = 0usize;
    let mut seen = 0usize;
    while seen < count {
        let Some((next, _)) = next_char(bytes, offset) else {
            break;
        };
        offset = next;
        seen += 1;
    }
    offset
}

/// Returns the byte length of the longest prefix whose display width is at most `budget`.
fn take_display_width(bytes: &[u8], budget: usize, byte_width: bool) -> usize {
    if byte_width {
        return budget.min(bytes.len());
    }
    let mut offset = 0usize;
    let mut remaining = budget;
    while let Some((next, codepoint)) = next_char(bytes, offset) {
        let width = character_width(codepoint);
        if remaining < width {
            break;
        }
        remaining -= width;
        offset = next;
    }
    offset
}

/// Walks one mbstring character: a valid scalar, one malformed sequence, or a truncated suffix.
fn next_char(bytes: &[u8], offset: usize) -> Option<(usize, u32)> {
    if offset >= bytes.len() {
        return None;
    }
    match std::str::from_utf8(&bytes[offset..]) {
        Ok(valid) => {
            let ch = valid.chars().next()?;
            Some((offset + ch.len_utf8(), ch as u32))
        }
        Err(error) => {
            let valid_len = error.valid_up_to();
            if valid_len > 0 {
                let valid = std::str::from_utf8(&bytes[offset..offset + valid_len])
                    .expect("from_utf8 valid prefix");
                let ch = valid.chars().next()?;
                return Some((offset + ch.len_utf8(), ch as u32));
            }
            match error.error_len() {
                Some(invalid_len) => Some((offset + invalid_len, 0xffff_ffff)),
                None => Some((bytes.len(), 0xffff_ffff)),
            }
        }
    }
}

/// Returns PHP mbstring display width: 2 inside the East Asian Width table, otherwise 1.
fn character_width(codepoint: u32) -> usize {
    if codepoint < 0x1100 {
        return 1;
    }
    let mut lo = 0usize;
    let mut hi = EAW_RANGES.len();
    while lo < hi {
        let probe = (lo + hi) / 2;
        let (begin, end) = EAW_RANGES[probe];
        if codepoint < begin {
            hi = probe;
        } else if codepoint > end {
            lo = probe + 1;
        } else {
            return 2;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::{character_width, trim_to_width, MbStrimwidthError};

    /// Verifies ASCII halfwidth characters and CJK fullwidth characters match PHP widths.
    #[test]
    fn character_width_matches_php_east_asian_table() {
        assert_eq!(character_width(b'a' as u32), 1);
        assert_eq!(character_width(0xff41), 2);
        assert_eq!(character_width(0x65e5), 2);
        assert_eq!(character_width(0x2026), 1);
    }

    /// Verifies PHP's trim-marker replacement and CJK width accounting.
    #[test]
    fn trim_to_width_matches_php_truncation_rules() {
        assert_eq!(
            trim_to_width(b"hello", 0, 3, b"...", false).unwrap(),
            b"...".to_vec()
        );
        assert_eq!(
            trim_to_width(b"hello", 0, 4, b"...", false).unwrap(),
            b"h...".to_vec()
        );
        assert_eq!(
            trim_to_width("日本語".as_bytes(), 0, 4, "…".as_bytes(), false).unwrap(),
            "日…".as_bytes()
        );
        assert_eq!(
            trim_to_width(b"hello", 1, 3, b"", false).unwrap(),
            b"ell".to_vec()
        );
        assert!(matches!(
            trim_to_width(b"ab", 3, 1, b"", false),
            Err(MbStrimwidthError::StartOutOfRange)
        ));
        assert_eq!(
            trim_to_width(b"hello", 0, -2, b"", false).unwrap(),
            b"hel".to_vec()
        );
        assert_eq!(trim_to_width(b"ab", 2, 1, b"...", false).unwrap(), b"".to_vec());
        assert!(matches!(
            trim_to_width(b"ab", 2, -1, b"", false),
            Err(MbStrimwidthError::WidthOutOfRange)
        ));
    }
}
