//! Purpose:
//! Defines PHP mbregex encoding aliases, option parsing, and per-call execution limits.
//!
//! Called from:
//! - The shared regex engine and future AOT/eval mbregex request adapters.
//!
//! Key details:
//! - Native encoding identity and PHP byte validation retain the original alias distinction.
//! - Options reject the first invalid byte and publish settings only after complete validation.

use crate::{encoding::Encoding, error::{MbError, MbResult}};

// Ordered like the opaque native provider's encoding IDs, from PHP 8.5.10 php_mbregex.c.
const ENCODINGS: &[(&str, &[&str])] = &[
    ("EUC-JP", &["EUC-JP", "EUCJP", "X-EUC-JP", "UJIS", "EUCJP", "EUCJP-WIN"]),
    ("UTF-8", &["UTF-8", "UTF8"]),
    ("UTF-16", &["UTF-16", "UTF-16BE"]),
    ("UTF-16LE", &["UTF-16LE"]),
    ("UCS-4", &["UCS-4", "UTF-32", "UTF-32BE"]),
    ("UCS-4LE", &["UCS-4LE", "UTF-32LE"]),
    ("SJIS", &["SJIS", "CP932", "MS932", "SHIFT_JIS", "SJIS-WIN", "WINDOWS-31J"]),
    ("BIG5", &["BIG5", "BIG-5", "BIGFIVE", "CN-BIG5", "BIG-FIVE"]),
    ("EUC-CN", &["EUC-CN", "EUCCN", "EUC_CN", "GB-2312", "GB2312"]),
    ("EUC-TW", &["EUC-TW", "EUCTW", "EUC_TW"]),
    ("EUC-KR", &["EUC-KR", "EUCKR", "EUC_KR"]),
    ("KOI8R", &["KOI8R", "KOI8-R", "KOI-8R"]),
    ("ISO-8859-1", &["ISO-8859-1", "ISO8859-1"]),
    ("ISO-8859-2", &["ISO-8859-2", "ISO8859-2"]),
    ("ISO-8859-3", &["ISO-8859-3", "ISO8859-3"]),
    ("ISO-8859-4", &["ISO-8859-4", "ISO8859-4"]),
    ("ISO-8859-5", &["ISO-8859-5", "ISO8859-5"]),
    ("ISO-8859-6", &["ISO-8859-6", "ISO8859-6"]),
    ("ISO-8859-7", &["ISO-8859-7", "ISO8859-7"]),
    ("ISO-8859-8", &["ISO-8859-8", "ISO8859-8"]),
    ("ISO-8859-9", &["ISO-8859-9", "ISO8859-9"]),
    ("ISO-8859-10", &["ISO-8859-10", "ISO8859-10"]),
    ("ISO-8859-11", &["ISO-8859-11", "ISO8859-11"]),
    ("ISO-8859-13", &["ISO-8859-13", "ISO8859-13"]),
    ("ISO-8859-14", &["ISO-8859-14", "ISO8859-14"]),
    ("ISO-8859-15", &["ISO-8859-15", "ISO8859-15"]),
    ("ISO-8859-16", &["ISO-8859-16", "ISO8859-16"]),
    ("ASCII", &["ASCII", "US-ASCII", "US_ASCII", "ISO646"]),
];

/// One Oniguruma encoding paired with the PHP validator selected by its original spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegexEncoding { pub(super) id: u32, validation: Encoding }

impl RegexEncoding {
    /// Resolves PHP's C-string aliases, rejecting names unsupported by either native regex or mbfl.
    pub fn lookup(name: &[u8]) -> Option<Self> {
        let name = name.split(|&byte| byte == 0).next()?;
        let id = ENCODINGS.iter().position(|(_, aliases)| aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias.as_bytes())))?;
        Some(Self { id: id as u32, validation: Encoding::lookup(name)? })
    }

    /// Resolves an INI native default using the canonical validator selected at request shutdown.
    pub(super) fn default_for_ini(name: &[u8]) -> Option<Self> {
        let name = name.split(|&byte| byte == 0).next()?;
        let id = ENCODINGS.iter().position(|(_, aliases)| aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias.as_bytes())))?;
        Some(Self { id: id as u32, validation: Encoding::lookup(ENCODINGS[id].0.as_bytes())? })
    }

    /// Returns the canonical mbregex name, which can differ from mbstring's encoding name.
    pub fn name(self) -> &'static str { ENCODINGS[self.id as usize].0 }

    /// Checks bytes using the PHP encoding retained when this regex encoding was selected.
    pub fn is_valid(self, bytes: &[u8]) -> bool { self.validation.decode(bytes).is_valid() }

    /// Reads PHP's raw replacement-character width without decoding or validating the replacement.
    pub(super) fn replacement_width(self, byte: u8) -> usize {
        if matches!(self.id, 2 | 3) { return 2; }
        if matches!(self.id, 4 | 5) { return 4; }
        match self.validation.slicing() {
            crate::encoding::Slicing::Fixed(width) => width,
            crate::encoding::Slicing::LeadingByte(table) => table[byte as usize] as usize,
            crate::encoding::Slicing::Converted => 1,
        }
    }
}

impl Default for RegexEncoding {
    /// Starts regex requests in PHP's UTF-8 encoding.
    fn default() -> Self { Self::lookup(b"UTF-8").expect("UTF-8 regex encoding") }
}

/// Parsed mbregex options, independent of native Oniguruma structure layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options { pub(super) bits: u32, pub(super) syntax: u32 }

impl Default for Options {
    /// Starts with PHP's multiline/singleline combination and Ruby syntax (`pr`).
    fn default() -> Self { Self { bits: 12, syntax: 0 } }
}

impl Options {
    /// Parses explicit option bytes, with optional forced case-insensitivity for eregi operations.
    pub fn parse(input: &[u8], ignore_case: bool) -> MbResult<Self> {
        let (options, error) = Self::parse_progressive(input, ignore_case);
        match error { Some(error) => Err(error), None => Ok(options) }
    }

    /// Retains the syntax but discards accumulated flags when PHP's progressive parser raises an error.
    pub(super) fn parse_progressive(input: &[u8], ignore_case: bool) -> (Self, Option<MbError>) {
        let mut result = Self { bits: u32::from(ignore_case), syntax: 0 };
        for &byte in input {
            match byte {
                b'i' => result.bits |= 1, b'x' => result.bits |= 2,
                b'm' => result.bits |= 4, b's' => result.bits |= 8,
                b'p' => result.bits |= 12, b'l' => result.bits |= 16, b'n' => result.bits |= 32,
                b'r' => result.syntax = 0, b'j' => result.syntax = 1,
                b'u' => result.syntax = 2, b'g' => result.syntax = 3,
                b'c' => result.syntax = 4, b'z' => result.syntax = 5,
                b'b' => result.syntax = 6, b'd' => result.syntax = 7,
                _ => {
                    let mut message = b"Option \"".to_vec();
                    if byte != 0 { message.push(byte); message.extend_from_slice(b"\" is not supported"); }
                    result.bits = u32::from(ignore_case);
                    return (result, Some(match String::from_utf8(message) {
                        Ok(message) => MbError::Value(message),
                        Err(error) => MbError::ValueBytes(error.into_bytes()),
                    }));
                }
            }
        }
        (result, None)
    }

    /// Returns PHP's canonical option order and its final syntax selector.
    pub fn as_string(self) -> String {
        let mut result = String::new();
        if self.bits & 1 != 0 { result.push('i'); }
        if self.bits & 2 != 0 { result.push('x'); }
        match self.bits & 12 { 12 => result.push('p'), 4 => result.push('m'), 8 => result.push('s'), _ => {} }
        if self.bits & 16 != 0 { result.push('l'); }
        if self.bits & 32 != 0 { result.push('n'); }
        result.push(b"rjugczbd"[self.syntax as usize] as char);
        result
    }
}

/// Per-call limits distinguish explicit zero from keeping Oniguruma's native default.
#[derive(Clone, Copy, Debug)]
pub struct Limits { pub stack: Option<u32>, pub retry: Option<u32> }

impl Limits {
    /// Preserves PHP's different integer-boundary checks for match versus search operations.
    pub fn from_ini(stack: i64, retry: i64, anchored: bool) -> Self {
        let limit = |value| u32::try_from(value).ok().filter(|&value| !anchored || (value > 0 && value < u32::MAX));
        Self { stack: limit(stack), retry: limit(retry) }
    }
}
