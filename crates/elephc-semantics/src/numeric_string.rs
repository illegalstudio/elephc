//! Purpose:
//! Owns the single host-side implementation of PHP's numeric-string grammar
//! (`_is_numeric_string_ex`), shared by every pass that has to answer "is this string
//! numeric, and does it spell an int or a float?".
//!
//! Called from:
//! - Compiler `optimize::fold::compare` for `zend_compare()` over scalar literals.
//! - Compiler `types::param_binding` for string-into-`int`/`float` parameter coercion.
//! - Compiler `types::checker::inference::ops` for numeric-string arithmetic operands.
//!
//! Key details:
//! - This is the compile-time twin of the runtime `__rt_php_num_scan` helper
//!   (`crate::codegen_support::runtime::strings::php_num_scan`); the two must agree byte
//!   for byte or a literal and a runtime value give different answers for the same
//!   operation. Keeping exactly one compile-time copy is what makes that auditable.
//! - Grammar: optional PHP whitespace, an optional sign, a mantissa with at least one
//!   digit (`12`, `.5`, `5.`), and an optional `e`/`E` exponent consumed only when a digit
//!   follows it (`"1e"` scans as `1`). There is NO hexadecimal form, NO underscore
//!   separator, and NO `INF`/`NAN` spelling ; those are libc `strtod` extensions PHP
//!   does not have.

/// Returns whether a byte is one of the six characters PHP's `ZEND_IS_WHITESPACE` accepts.
///
/// PHP allows these on both sides of a numeric string, so `" 42 "` is numeric while
/// `"42x"` is only leading-numeric.
pub fn is_php_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// The longest leading numeric run of a string, as scanned by PHP's `_is_numeric_string_ex`.
pub struct NumericScan<'a> {
    /// The matched numeric text with leading whitespace and trailing garbage removed.
    pub text: &'a str,
    /// Whether the run contained a decimal point or a consumed exponent (`IS_DOUBLE` syntax).
    pub is_float: bool,
    /// Everything after the matched run, still un-trimmed.
    pub trailing: &'a str,
}

impl NumericScan<'_> {
    /// Returns whether the run covers the whole string, i.e. PHP's `is_numeric()` is `true`.
    ///
    /// Only PHP whitespace may follow the run; any other trailing byte makes the string
    /// merely *leading*-numeric, which arithmetic accepts with a warning and parameter
    /// coercion rejects.
    pub fn is_fully_numeric(&self) -> bool {
        self.trailing.bytes().all(is_php_whitespace)
    }
}

/// Scans the longest leading numeric run of `value` using PHP's numeric-string grammar.
///
/// Accepts optional leading whitespace, an optional sign, a mantissa with at least one
/// digit (`12`, `.5`, `5.`), and an exponent only when at least one digit follows it ;
/// `"1e"` therefore scans as `1` with `"e"` left over, exactly like PHP. Hex, underscore
/// separators, `INF` and `NAN` are not part of the grammar. Returns `None` when no digit
/// is present at all.
pub fn scan_numeric_prefix(value: &str) -> Option<NumericScan<'_>> {
    let bytes = value.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && is_php_whitespace(bytes[idx]) {
        idx += 1;
    }

    let start = idx;
    if idx < bytes.len() && matches!(bytes[idx], b'+' | b'-') {
        idx += 1;
    }

    let mut digits = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
        digits += 1;
    }

    let mut is_float = false;
    if idx < bytes.len() && bytes[idx] == b'.' {
        let mut probe = idx + 1;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
            digits += 1;
        }
        if digits > 0 {
            idx = probe;
            is_float = true;
        }
    }
    if digits == 0 {
        return None;
    }

    if idx < bytes.len() && matches!(bytes[idx], b'e' | b'E') {
        let mut probe = idx + 1;
        if probe < bytes.len() && matches!(bytes[probe], b'+' | b'-') {
            probe += 1;
        }
        let exponent_start = probe;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
        }
        if probe > exponent_start {
            idx = probe;
            is_float = true;
        }
    }

    Some(NumericScan {
        text: &value[start..idx],
        is_float,
        trailing: &value[idx..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the fully-numeric spellings PHP's `is_numeric()` accepts, including the
    /// PHP 8 trailing-whitespace rule and the `5.` / `.5` mantissa forms.
    #[test]
    fn fully_numeric_spellings() {
        for (value, is_float) in [
            ("123", false),
            ("-123", false),
            ("+123", false),
            (" 42 ", false),
            ("\n42\t", false),
            ("1.5", true),
            ("5.", true),
            (".5", true),
            ("1e3", true),
            ("1E-3", true),
        ] {
            let scan = scan_numeric_prefix(value).expect(value);
            assert!(scan.is_fully_numeric(), "{value} should be fully numeric");
            assert_eq!(scan.is_float, is_float, "{value} float classification");
        }
    }

    /// Verifies the leading-numeric spellings: PHP consumes the numeric prefix and treats
    /// the rest as trailing garbage. `"0x1A"`, `"1_000"` and `"1e"` are the libc-only
    /// spellings PHP's grammar deliberately rejects.
    #[test]
    fn leading_numeric_spellings() {
        for (value, text, is_float) in [
            ("12abc", "12", false),
            ("  +12foo", "+12", false),
            ("0x1A", "0", false),
            ("1_000", "1", false),
            ("1e", "1", false),
            ("1.5e", "1.5", true),
            ("12.foo", "12.", true),
        ] {
            let scan = scan_numeric_prefix(value).expect(value);
            assert!(!scan.is_fully_numeric(), "{value} should not be fully numeric");
            assert_eq!(scan.text, text, "{value} numeric run");
            assert_eq!(scan.is_float, is_float, "{value} float classification");
        }
    }

    /// Verifies the strings with no numeric prefix at all, which PHP rejects outright.
    #[test]
    fn non_numeric_spellings() {
        for value in ["", "hi", ".", "-", "+", "e5", "INF", "NAN", " ", "-.e3"] {
            assert!(
                scan_numeric_prefix(value).is_none(),
                "{value} should have no numeric prefix"
            );
        }
    }
}
