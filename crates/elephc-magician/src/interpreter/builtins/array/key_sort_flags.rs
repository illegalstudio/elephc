//! Purpose:
//! The key comparisons `ksort()`/`krsort()` select with PHP's `$flags` argument, for eval.
//!
//! Called from:
//! - `crate::interpreter::builtins::array::sort`.
//!
//! Key details:
//! - `SORT_REGULAR` is not implemented here. It keeps the existing native comparator, so a
//!   flagless key sort runs exactly the code it ran before `$flags` existed.
//! - Every byte-comparing mode spells an integer key out as decimal digits first, which is what
//!   php-src does with `zend_print_long_to_buf`.
//! - The natural-order comparison is a direct port of php-src's `strnatcmp_ex`, quirks included.
//!   It folds ASCII case only: php-src calls libc `toupper()` there, which folds Latin-1 under
//!   Darwin's C locale but not under glibc's, and elephc gives the same answer on every target.
//! - `SORT_LOCALE_STRING` clips both operands at the first NUL and compares bytes, which is
//!   `strcoll` in the C locale -- the only locale a PHP program without `setlocale()` is in.

use std::cmp::Ordering;

use super::super::super::*;

/// The key comparison PHP selects from `ksort()`/`krsort()`'s `$flags`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::interpreter) enum EvalKeySortMode {
    /// PHP's default: the same table `<` and `<=>` use.
    Regular,
    /// `SORT_NUMERIC`.
    Numeric,
    /// `SORT_STRING`, optionally with `SORT_FLAG_CASE`.
    Binary { fold_case: bool },
    /// `SORT_LOCALE_STRING`.
    Locale,
    /// `SORT_NATURAL`, optionally with `SORT_FLAG_CASE`.
    Natural { fold_case: bool },
}

/// Resolves PHP's `$flags` word the way `php_get_key_compare_func` does.
///
/// Every value outside the four PHP recognizes -- including `3`, `4` and `999` -- falls back to
/// `SORT_REGULAR` rather than raising.
pub(in crate::interpreter) fn eval_key_sort_mode(flags: i64) -> EvalKeySortMode {
    let fold_case = flags & 8 != 0;
    match flags & !8 {
        1 => EvalKeySortMode::Numeric,
        2 => EvalKeySortMode::Binary { fold_case },
        5 => EvalKeySortMode::Locale,
        6 => EvalKeySortMode::Natural { fold_case },
        _ => EvalKeySortMode::Regular,
    }
}

/// Orders two normalized array keys under `mode`.
pub(in crate::interpreter) fn eval_key_sort_compare(
    mode: EvalKeySortMode,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Ordering, EvalStatus> {
    if mode == EvalKeySortMode::Numeric {
        return eval_numeric_key_compare(left, right, values);
    }
    let left = eval_key_bytes(left, values)?;
    let right = eval_key_bytes(right, values)?;
    Ok(match mode {
        EvalKeySortMode::Regular | EvalKeySortMode::Numeric => Ordering::Equal,
        EvalKeySortMode::Binary { fold_case: false } => left.cmp(&right),
        EvalKeySortMode::Binary { fold_case: true } => {
            let mut left = left;
            let mut right = right;
            left.make_ascii_lowercase();
            right.make_ascii_lowercase();
            left.cmp(&right)
        }
        EvalKeySortMode::Locale => eval_clip_nul(&left).cmp(eval_clip_nul(&right)),
        EvalKeySortMode::Natural { fold_case } => eval_strnatcmp(&left, &right, fold_case),
    })
}

/// Orders two keys under `SORT_NUMERIC`.
///
/// Two integer keys compare exactly rather than through a double, and PHP never reports them
/// equal: a hash cannot hold the same key twice.
fn eval_numeric_key_compare(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Ordering, EvalStatus> {
    if let (Some(left), Some(right)) = (
        eval_key_int(left, values)?,
        eval_key_int(right, values)?,
    ) {
        return Ok(if left > right {
            Ordering::Greater
        } else {
            Ordering::Less
        });
    }
    let left = eval_key_numeric(left, values)?;
    let right = eval_key_numeric(right, values)?;
    Ok(left.partial_cmp(&right).unwrap_or(Ordering::Greater))
}

/// Returns the integer payload of an integer key, or `None` for a string key.
fn eval_key_int(
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, EvalStatus> {
    match values.type_tag(key)? {
        EVAL_TAG_INT => Ok(Some(eval_int_value(key, values)?)),
        _ => Ok(None),
    }
}

/// Returns the bytes PHP would compare for one normalized array key.
fn eval_key_bytes(
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    match values.type_tag(key)? {
        EVAL_TAG_INT => Ok(eval_int_value(key, values)?.to_string().into_bytes()),
        EVAL_TAG_STRING => values.string_bytes(key),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the double PHP would compare for one normalized array key.
fn eval_key_numeric(
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<f64, EvalStatus> {
    match values.type_tag(key)? {
        EVAL_TAG_INT => Ok(eval_int_value(key, values)? as f64),
        EVAL_TAG_STRING => Ok(eval_php_numeric_prefix(&values.string_bytes(key)?)),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads the value of a string's leading numeric run, or `0.0` when it has none.
///
/// The grammar is PHP's, not libc's: no hexadecimal, no `INF`/`NAN` spelling, and an exponent
/// only when at least one digit follows it, so `"0x10"`, `"INF"` and `"1e"` are `0.0`, `0.0`
/// and `1.0`.
pub(in crate::interpreter) fn eval_php_numeric_prefix(bytes: &[u8]) -> f64 {
    let mut index = 0;
    while index < bytes.len() && eval_is_php_space(bytes[index]) {
        index += 1;
    }
    let start = index;
    if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
        index += 1;
    }
    let mut digits = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        digits += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return 0.0;
    }
    let mantissa_end = index;
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        let mut exponent = index + 1;
        if exponent < bytes.len() && matches!(bytes[exponent], b'+' | b'-') {
            exponent += 1;
        }
        let exponent_start = exponent;
        while exponent < bytes.len() && bytes[exponent].is_ascii_digit() {
            exponent += 1;
        }
        if exponent > exponent_start {
            index = exponent;
        } else {
            index = mantissa_end;
        }
    }
    std::str::from_utf8(&bytes[start..index])
        .ok()
        .and_then(|run| run.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Reports whether `byte` is one of the six characters PHP treats as whitespace.
fn eval_is_php_space(byte: u8) -> bool {
    byte == b' ' || (0x09..=0x0d).contains(&byte)
}

/// Returns the prefix `strcoll` would see, which ends at the first NUL.
fn eval_clip_nul(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|byte| *byte == 0) {
        Some(end) => &bytes[..end],
        None => bytes,
    }
}

/// Compares two byte strings in PHP's natural order, a port of php-src's `strnatcmp_ex`.
pub(in crate::interpreter) fn eval_strnatcmp(a: &[u8], b: &[u8], fold_case: bool) -> Ordering {
    if a.is_empty() || b.is_empty() {
        return a.len().cmp(&b.len());
    }
    let mut ap = 0usize;
    let mut bp = 0usize;
    let mut ca = a[0];
    let mut cb = b[0];

    // Leading zeros are skipped once, before the loop, not per digit run.
    while ca == b'0' && ap + 1 < a.len() && a[ap + 1].is_ascii_digit() {
        ap += 1;
        ca = a[ap];
    }
    while cb == b'0' && bp + 1 < b.len() && b[bp + 1].is_ascii_digit() {
        bp += 1;
        cb = b[bp];
    }

    loop {
        // PHP reads the NUL its strings always carry when a skip walks off the end.
        while eval_is_php_space(ca) {
            ap += 1;
            ca = a.get(ap).copied().unwrap_or(0);
        }
        while eval_is_php_space(cb) {
            bp += 1;
            cb = b.get(bp).copied().unwrap_or(0);
        }

        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let result = if ca == b'0' || cb == b'0' {
                eval_compare_left(a, &mut ap, b, &mut bp)
            } else {
                eval_compare_right(a, &mut ap, b, &mut bp)
            };
            if result != Ordering::Equal {
                return result;
            }
            if ap == a.len() && bp == b.len() {
                return Ordering::Equal;
            } else if ap == a.len() {
                return Ordering::Less;
            } else if bp == b.len() {
                return Ordering::Greater;
            }
            ca = a[ap];
            cb = b[bp];
        }

        let (left, right) = if fold_case {
            (ca.to_ascii_uppercase(), cb.to_ascii_uppercase())
        } else {
            (ca, cb)
        };
        if left != right {
            return left.cmp(&right);
        }

        ap += 1;
        bp += 1;
        if ap >= a.len() && bp >= b.len() {
            return Ordering::Equal;
        } else if ap >= a.len() {
            return Ordering::Less;
        } else if bp >= b.len() {
            return Ordering::Greater;
        }
        ca = a[ap];
        cb = b[bp];
    }
}

/// Compares two digit runs by length first: `"img10"` sorts after `"img9"`.
fn eval_compare_right(a: &[u8], ap: &mut usize, b: &[u8], bp: &mut usize) -> Ordering {
    let mut bias = Ordering::Equal;
    loop {
        let left = a.get(*ap).copied().filter(u8::is_ascii_digit);
        let right = b.get(*bp).copied().filter(u8::is_ascii_digit);
        match (left, right) {
            (None, None) => return bias,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left), Some(right)) => {
                if bias == Ordering::Equal {
                    bias = left.cmp(&right);
                }
            }
        }
        *ap += 1;
        *bp += 1;
    }
}

/// Compares two fractional digit runs by their first differing digit.
fn eval_compare_left(a: &[u8], ap: &mut usize, b: &[u8], bp: &mut usize) -> Ordering {
    loop {
        let left = a.get(*ap).copied().filter(u8::is_ascii_digit);
        let right = b.get(*bp).copied().filter(u8::is_ascii_digit);
        match (left, right) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left), Some(right)) => {
                if left != right {
                    return left.cmp(&right);
                }
            }
        }
        *ap += 1;
        *bp += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the natural-order port against values taken from PHP 8.5's own `strnatcmp()`.
    ///
    /// The surprising ones are the point: two operands that differ only in skipped bytes compare
    /// EQUAL (`"a 7"` vs `"a7"`, `"0"` vs `"00"`), a run that starts with a zero on either side
    /// compares left-aligned (`"a0.5"` before `"a0.10"`), and any other run compares by length
    /// first (`"img10"` after `"img9"`).
    #[test]
    fn natural_order_matches_php_strnatcmp() {
        let cases: &[(&str, &str, Ordering)] = &[
            ("a 7", "a7", Ordering::Equal),
            ("  1", "1", Ordering::Equal),
            ("0", "00", Ordering::Equal),
            ("1", "01", Ordering::Equal),
            ("a0.5", "a0.10", Ordering::Less),
            ("a007", "a7", Ordering::Less),
            ("a1b", "a01b", Ordering::Greater),
            ("img12", "img2", Ordering::Greater),
            ("img10", "img9", Ordering::Greater),
            ("2", "10", Ordering::Less),
            ("a2", "a10", Ordering::Less),
            ("x", "x0", Ordering::Less),
            ("z1", "z", Ordering::Greater),
            ("A", "a", Ordering::Less),
            ("", "a", Ordering::Less),
            ("", "", Ordering::Equal),
            ("a\0b", "a\0a", Ordering::Greater),
        ];
        for (left, right, expected) in cases {
            assert_eq!(
                eval_strnatcmp(left.as_bytes(), right.as_bytes(), false),
                *expected,
                "strnatcmp({left:?}, {right:?})"
            );
        }
    }

    /// Verifies the case-folded variant against PHP 8.5's own `strnatcasecmp()`.
    #[test]
    fn natural_order_folds_ascii_case_like_php() {
        assert_eq!(eval_strnatcmp(b"A", b"a", true), Ordering::Equal);
        assert_eq!(eval_strnatcmp(b"IMG1", b"img1", true), Ordering::Equal);
        assert_eq!(eval_strnatcmp(b"a0.5", b"A0.10", true), Ordering::Less);
        // Folding is ASCII-only, so a byte above 127 keeps its own value on every target.
        assert_eq!(eval_strnatcmp(b"\xff", b"\x80", true), Ordering::Greater);
    }

    /// Verifies the numeric-prefix reader follows PHP's grammar rather than libc's.
    ///
    /// `zend_strtod` has no hexadecimal, no `INF`/`NAN` spelling, and consumes an exponent only
    /// when a digit follows it, which is why three of these are zero and `"1e"` is one.
    #[test]
    fn numeric_prefix_matches_php_key_values() {
        let cases: &[(&str, f64)] = &[
            ("0x10", 0.0),
            ("INF", 0.0),
            ("NAN", 0.0),
            ("abc", 0.0),
            ("1e", 1.0),
            ("1e2", 100.0),
            (".5", 0.5),
            ("+5", 5.0),
            ("05", 5.0),
            (" 5", 5.0),
            ("\n5", 5.0),
            ("\t5", 5.0),
            ("12abc", 12.0),
            ("-3.25", -3.25),
            ("", 0.0),
        ];
        for (input, expected) in cases {
            assert_eq!(
                eval_php_numeric_prefix(input.as_bytes()),
                *expected,
                "numeric prefix of {input:?}"
            );
        }
        assert!(eval_php_numeric_prefix(b"1e999").is_infinite());
    }

    /// Verifies PHP's flag resolution, including the values it silently ignores.
    #[test]
    fn flag_resolution_matches_php_selection() {
        assert!(eval_key_sort_mode(0) == EvalKeySortMode::Regular);
        assert!(eval_key_sort_mode(1) == EvalKeySortMode::Numeric);
        assert!(eval_key_sort_mode(2) == EvalKeySortMode::Binary { fold_case: false });
        assert!(eval_key_sort_mode(2 | 8) == EvalKeySortMode::Binary { fold_case: true });
        assert!(eval_key_sort_mode(5) == EvalKeySortMode::Locale);
        assert!(eval_key_sort_mode(6) == EvalKeySortMode::Natural { fold_case: false });
        assert!(eval_key_sort_mode(6 | 8) == EvalKeySortMode::Natural { fold_case: true });
        // SORT_FLAG_CASE alone, SORT_DESC, SORT_ASC and an unknown word all mean SORT_REGULAR.
        for flags in [8, 3, 4, 7, 999, -1] {
            assert!(
                eval_key_sort_mode(flags) == EvalKeySortMode::Regular,
                "{flags}"
            );
        }
    }
}
