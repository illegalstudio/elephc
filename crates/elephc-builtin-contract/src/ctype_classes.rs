//! Purpose:
//! The byte classes behind PHP's `ctype_*` predicates, per platform.
//!
//! Called from:
//! - `src/codegen_support/runtime/data/fixed.rs`, which emits the table compiled code indexes.
//! - The eval interpreter's ctype hook, which answers from the same table.
//!
//! Key details:
//! - php asks libc, under the `LC_CTYPE=C.UTF-8` it selects at startup. Bytes below 0x80 are ASCII
//!   everywhere. Above, glibc classifies nothing, but darwin reads the byte as a Latin-1 code point,
//!   so on macOS `ctype_alpha("é")` is true and one table cannot serve both.
//! - `DARWIN_HIGH_BYTES` was captured from php 8.5.10 on macOS, byte by byte.

/// `ctype_alnum()`.
pub const CTYPE_ALNUM: u16 = 1 << 0;
/// `ctype_alpha()`.
pub const CTYPE_ALPHA: u16 = 1 << 1;
/// `ctype_cntrl()`.
pub const CTYPE_CNTRL: u16 = 1 << 2;
/// `ctype_digit()`.
pub const CTYPE_DIGIT: u16 = 1 << 3;
/// `ctype_graph()`.
pub const CTYPE_GRAPH: u16 = 1 << 4;
/// `ctype_lower()`.
pub const CTYPE_LOWER: u16 = 1 << 5;
/// `ctype_print()`.
pub const CTYPE_PRINT: u16 = 1 << 6;
/// `ctype_punct()`.
pub const CTYPE_PUNCT: u16 = 1 << 7;
/// `ctype_space()`.
pub const CTYPE_SPACE: u16 = 1 << 8;
/// `ctype_upper()`.
pub const CTYPE_UPPER: u16 = 1 << 9;
/// `ctype_xdigit()`.
pub const CTYPE_XDIGIT: u16 = 1 << 10;

/// The class bits darwin's libc gives bytes 0x80..=0xff under `C.UTF-8`.
const DARWIN_HIGH_BYTES: [u16; 128] = [
    0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004,
    0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004,
    0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004,
    0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004, 0x004,
    0x140, 0x0d0, 0x050, 0x050, 0x050, 0x050, 0x050, 0x0d0,
    0x050, 0x050, 0x053, 0x0d0, 0x050, 0x004, 0x050, 0x050,
    0x050, 0x050, 0x050, 0x050, 0x050, 0x073, 0x0d0, 0x0d0,
    0x050, 0x050, 0x053, 0x0d0, 0x050, 0x050, 0x050, 0x0d0,
    0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x253,
    0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x253,
    0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x050,
    0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x253, 0x073,
    0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x073,
    0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x073,
    0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x050,
    0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x073, 0x073,
];

/// The class bits of one byte on darwin (`true`) or on any other libc (`false`).
pub fn ctype_class_bits(byte: u8, darwin: bool) -> u16 {
    if byte >= 0x80 {
        return if darwin { DARWIN_HIGH_BYTES[usize::from(byte - 0x80)] } else { 0 };
    }
    let mut bits = 0;
    for (holds, class) in [
        (byte.is_ascii_alphanumeric(), CTYPE_ALNUM),
        (byte.is_ascii_alphabetic(), CTYPE_ALPHA),
        (byte.is_ascii_control(), CTYPE_CNTRL),
        (byte.is_ascii_digit(), CTYPE_DIGIT),
        (byte.is_ascii_graphic(), CTYPE_GRAPH),
        (byte.is_ascii_lowercase(), CTYPE_LOWER),
        (matches!(byte, 0x20..=0x7e), CTYPE_PRINT),
        (byte.is_ascii_punctuation(), CTYPE_PUNCT),
        (matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'), CTYPE_SPACE),
        (byte.is_ascii_uppercase(), CTYPE_UPPER),
        (byte.is_ascii_hexdigit(), CTYPE_XDIGIT),
    ] {
        if holds {
            bits |= class;
        }
    }
    bits
}

/// The class bit of a `ctype_*` builtin, by its PHP name.
pub fn ctype_class_of(name: &str) -> Option<u16> {
    Some(match name {
        "ctype_alnum" => CTYPE_ALNUM,
        "ctype_alpha" => CTYPE_ALPHA,
        "ctype_cntrl" => CTYPE_CNTRL,
        "ctype_digit" => CTYPE_DIGIT,
        "ctype_graph" => CTYPE_GRAPH,
        "ctype_lower" => CTYPE_LOWER,
        "ctype_print" => CTYPE_PRINT,
        "ctype_punct" => CTYPE_PUNCT,
        "ctype_space" => CTYPE_SPACE,
        "ctype_upper" => CTYPE_UPPER,
        "ctype_xdigit" => CTYPE_XDIGIT,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the class populations php reports, on both libc families.
    #[test]
    fn class_populations_match_php() {
        let population = |class: u16, darwin: bool| {
            (0..=255u8).filter(|byte| ctype_class_bits(*byte, darwin) & class != 0).count()
        };
        assert_eq!(population(CTYPE_UPPER, true), 56);
        assert_eq!(population(CTYPE_LOWER, true), 59);
        assert_eq!(population(CTYPE_PUNCT, true), 39);
        assert_eq!(population(CTYPE_CNTRL, true), 66);
        assert_eq!(population(CTYPE_PRINT, true), 190);
        assert_eq!(population(CTYPE_UPPER, false), 26);
        assert_eq!(population(CTYPE_PRINT, false), 95);
    }
}
