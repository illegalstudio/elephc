//! Purpose:
//! Finds numeric literal boundaries for each Chopper scanner consumer.
//!
//! Called from:
//! - The PHP code scanner and the interpolation offset scanner.
//!
//! Key details:
//! - The compiler keeps malformed prefixes and exponents together for its diagnostics.
//! - PHP keeps its token boundaries even when syntax validation will reject the source.

use super::lexeme::{digit_in_radix, digits_len};

/// Numeric boundary rules selected by the scanner's caller.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberProfile {
    Zend,
    Runtime,
    Compiler,
    Validation,
}

/// The interpretation of a numeric lexeme, before converting its value.
#[derive(Clone, Copy)]
pub(crate) enum NumberKind {
    Decimal,
    LegacyOctal,
    Prefixed(u32),
    Float,
}

/// One numeric lexeme starting at offset zero.
pub(crate) struct Number {
    /// The conversion rules for the scanned text.
    pub(crate) kind: NumberKind,
    /// The number of source bytes consumed.
    pub(crate) len: usize,
}

/// Scans a digit- or dot-led number without allocating or converting its value.
pub(crate) fn scan_number(source: &[u8], profile: NumberProfile) -> Number {
    let compiler = profile == NumberProfile::Compiler;
    if let Some(radix) = numeric_prefix(source, compiler) {
        return Number { kind: NumberKind::Prefixed(radix), len: 2 + digits_len(&source[2..], radix) };
    }

    let starts_with_dot = source[0] == b'.';
    let mut len = digits_len(source, 10);
    let fraction = source.get(len) == Some(&b'.')
        && (profile == NumberProfile::Zend || source.get(len + 1).is_some_and(u8::is_ascii_digit));
    if fraction {
        len += 1;
        len += digits_len(&source[len..], 10);
    }
    // The compiler's validation view historically consumes a malformed exponent
    // after a digit-led fraction, even though it splits it off after an integer.
    let consume_empty_exponent = compiler
        || (profile == NumberProfile::Validation && fraction && !starts_with_dot);
    let exponent = exponent_end(source, len, consume_empty_exponent);
    if fraction || exponent.is_some() {
        return Number { kind: NumberKind::Float, len: exponent.unwrap_or(len) };
    }

    let kind = if len > 1 && source[0] == b'0' { NumberKind::LegacyOctal } else { NumberKind::Decimal };
    Number { kind, len }
}

/// Returns the radix of a recognized prefix, optionally retaining a prefix with no digits.
pub(super) fn numeric_prefix(source: &[u8], allow_empty: bool) -> Option<u32> {
    let [b'0', marker, rest @ ..] = source else { return None };
    let radix = match *marker {
        b'x' | b'X' => 16,
        b'o' | b'O' => 8,
        b'b' | b'B' => 2,
        _ => return None,
    };
    (allow_empty || rest.first().is_some_and(|&byte| digit_in_radix(byte, radix))).then_some(radix)
}

/// Returns the end of an exponent after its marker, optional sign and digits.
fn exponent_end(source: &[u8], at: usize, allow_empty: bool) -> Option<usize> {
    if !matches!(source.get(at), Some(b'e' | b'E')) {
        return None;
    }
    let mut digits = at + 1;
    if matches!(source.get(digits), Some(b'+' | b'-')) {
        digits += 1;
    }
    let end = digits + digits_len(&source[digits..], 10);
    (allow_empty || end > digits).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each consumer keeps its existing numeric boundaries in the shared scanner.
    #[test]
    fn numeric_boundaries_follow_the_source_profile() {
        let cases: &[(&[u8], [usize; 4])] = &[
            (b"1.e3", [4, 1, 1, 1]),
            (b"0xZ", [1, 1, 2, 1]),
            (b"1.2e+", [3, 3, 5, 5]),
            (b".5e+", [2, 2, 4, 2]),
            (b"1e+", [1, 1, 3, 1]),
            (b"0b1_0_name", [5, 5, 5, 5]),
        ];
        let profiles = [
            NumberProfile::Zend,
            NumberProfile::Runtime,
            NumberProfile::Compiler,
            NumberProfile::Validation,
        ];
        for &(source, expected) in cases {
            assert_eq!(profiles.map(|profile| scan_number(source, profile).len), expected, "{source:?}");
        }
    }
}
