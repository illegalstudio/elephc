//! Purpose:
//! Owns one libgmp `mpz_t` and the exact PHP-facing conversions around it.
//!
//! Called from:
//! - `crate::ops` for every arithmetic and number-theory operation.
//! - `crate::abi` for parse/format/intval/double entry points.
//!
//! Key details:
//! - `Mpz` initializes in `new` and clears in `Drop`; no operation exposes the raw
//!   pointer beyond the FFI call that consumes it.
//! - Decimal strings are the bridge's transport format. `parse` mirrors php-src's
//!   `convert_zstr_to_gmp`, including leading whitespace and 0x/0o/0b prefixes.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use crate::ffi::*;

/// One owned GMP integer.
pub struct Mpz {
    raw: MpzRaw,
}

impl Mpz {
    /// Creates a zero-initialized GMP integer.
    pub fn new() -> Self {
        let mut raw = MpzRaw { alloc: 0, size: 0, data: std::ptr::null_mut() };
        unsafe { __gmpz_init(&mut raw) };
        Self { raw }
    }

    /// Creates an integer from a signed 64-bit value.
    pub fn from_i64(value: i64) -> Self {
        let mut result = Self::new();
        unsafe { __gmpz_set_si(&mut result.raw, value as u64) };
        result
    }

    /// Parses a canonical decimal string produced by this bridge.
    pub fn from_decimal(value: &str) -> Result<Self, String> {
        Self::parse_base(value, 10)
    }

    /// Parses `value` with an explicit GMP base (0 selects C-style prefixes).
    pub fn parse_base(value: &str, base: i64) -> Result<Self, String> {
        let text = CString::new(value.as_bytes()).map_err(|_| "NUL in integer string".to_string())?;
        let mut result = Self::new();
        let status = unsafe { __gmpz_set_str(&mut result.raw, text.as_ptr(), base as c_int) };
        if status == 0 {
            Ok(result)
        } else {
            Err("Number is not an integer string".to_string())
        }
    }

    /// Parses a PHP integer string with php-src's prefix rules.
    pub fn parse_php(value: &str, base: i64) -> Result<Self, String> {
        let trimmed = value.trim_matches(|ch: char| ch.is_ascii_whitespace());
        if trimmed.is_empty() {
            return Err("Number is not an integer string".to_string());
        }
        let mut effective = base;
        let mut digits = trimmed;
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 2 && bytes[0] == b'0' {
            let prefix = bytes[1].to_ascii_lowercase();
            if (base == 0 || base == 16) && prefix == b'x' {
                effective = 16;
                digits = &trimmed[2..];
            } else if (base == 0 || base == 8) && prefix == b'o' {
                effective = 8;
                digits = &trimmed[2..];
            } else if (base == 0 || base == 2) && prefix == b'b' {
                effective = 2;
                digits = &trimmed[2..];
            }
        }
        // `mpz_set_str` treats an empty string as zero, but PHP rejects a prefix with no
        // digits behind it (`"0x"`, `"0b"`, `"0o"`), and the empty string too.
        if digits.is_empty() {
            return Err("Number is not an integer string".to_string());
        }
        Self::parse_base(digits, effective)
    }

    /// Returns the value as a decimal string.
    pub fn to_decimal(&self) -> String {
        self.to_base(10).expect("base 10 is always valid")
    }

    /// Formats the value in one GMP base, including PHP's negative uppercase bases.
    pub fn to_base(&self, base: i64) -> Result<String, String> {
        if base == 0 || base == -1 || base == 1 || base > 62 || base < -36 {
            return Err("invalid GMP output base".to_string());
        }
        let digits = unsafe { __gmpz_sizeinbase(&self.raw, base.unsigned_abs() as c_int) };
        let mut buffer = vec![0u8; digits + 3];
        let written = unsafe {
            __gmpz_get_str(buffer.as_mut_ptr() as *mut c_char, base as c_int, &self.raw)
        };
        if written.is_null() {
            return Err("GMP could not format the number".to_string());
        }
        let text = unsafe { CStr::from_ptr(buffer.as_ptr() as *const c_char) };
        text.to_str()
            .map(|value| value.to_string())
            .map_err(|_| "GMP produced non-UTF-8 output".to_string())
    }

    /// Reports whether the value is zero.
    pub fn is_zero(&self) -> bool {
        self.raw.size == 0
    }

    /// Returns -1, 0 or 1 for the value's sign.
    pub fn sign(&self) -> i32 {
        if self.raw.size < 0 {
            -1
        } else if self.raw.size > 0 {
            1
        } else {
            0
        }
    }

    /// Returns the low signed machine word, matching php-src's `mpz_get_si`.
    pub fn to_i64(&self) -> i64 {
        unsafe { __gmpz_get_si(&self.raw) as i64 }
    }

    /// Returns the unsigned machine word, matching php-src's `mpz_get_ui`.
    pub fn to_u64(&self) -> u64 {
        unsafe { __gmpz_get_ui(&self.raw) as u64 }
    }

    /// Returns the nearest f64, matching libgmp's `mpz_get_d`.
    pub fn to_f64(&self) -> f64 {
        unsafe { __gmpz_get_d(&self.raw) }
    }

    /// Returns the pointer the FFI layer expects for `mpz_srcptr`.
    pub fn as_ptr(&self) -> *const MpzRaw {
        &self.raw
    }

    /// Returns the pointer the FFI layer expects for `mpz_ptr`.
    pub fn as_mut_ptr(&mut self) -> *mut MpzRaw {
        &mut self.raw
    }

    /// Returns the number of bits in the magnitude, matching `mpz_sizeinbase(_, 2)`.
    pub fn bit_length(&self) -> usize {
        unsafe { __gmpz_sizeinbase(&self.raw, 2) }
    }
}

impl Drop for Mpz {
    /// Releases the libgmp limbs owned by this value.
    fn drop(&mut self) {
        unsafe { __gmpz_clear(&mut self.raw) };
    }
}

/// Decodes the lowercase/uppercase hex transport used by `gmp_import`.
pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("odd hex payload".to_string());
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let raw = hex.as_bytes();
    for pair in raw.chunks_exact(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes.push(high << 4 | low);
    }
    Ok(bytes)
}

/// Encodes bytes as the lowercase hex transport used by `gmp_export`.
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Decodes one ASCII hex digit.
fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("invalid hex payload".to_string()),
    }
}
