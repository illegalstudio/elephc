//! Purpose:
//! The parts of PHP's standard library extensions call directly: base64,
//! rounding and number formatting, the string helpers of `php_string.h`,
//! randomness, temporary files and directory listing, the incomplete class,
//! the SAPI queries, and `phpinfo()` output (which prints nothing).
//!
//! Called from:
//! - Extension code.
//!
//! Key details:
//! - Behaviour follows PHP 8.5's ext/standard for the inputs extensions pass;
//!   `round()` rounds the decimal a float stands for (its shortest round-trip
//!   form), as PHP 8.4 started doing, so `round(0.285, 2)` is `0.29`.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

use crate::abi::*;
use crate::alloc::alloc;
use crate::global::Global;
use crate::string::{self, bytes};
use crate::zval::*;

// ------------------------------------------------------------ base64

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64-encodes `data` with the standard alphabet, appending `=` padding only when `padding` is
/// true.
pub fn base64_encode(data: &[u8], padding: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(BASE64[(n >> 18) as usize & 63]);
        out.push(BASE64[(n >> 12) as usize & 63]);
        if chunk.len() > 1 {
            out.push(BASE64[(n >> 6) as usize & 63]);
        } else if padding {
            out.push(b'=');
        }
        if chunk.len() > 2 {
            out.push(BASE64[n as usize & 63]);
        } else if padding {
            out.push(b'=');
        }
    }
    out
}

/// PHP's decoder: non-strict skips characters outside the alphabet; strict
/// refuses them and malformed padding.
pub fn base64_decode(data: &[u8], strict: bool) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut padding = 0;
    let mut seen = 0usize;
    for &c in data {
        if c == b'=' {
            padding += 1;
            continue;
        }
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => {
                if strict && !c.is_ascii_whitespace() {
                    return None;
                }
                continue;
            }
        };
        if strict && padding > 0 {
            return None;
        }
        acc = (acc << 6) | value as u32;
        bits += 6;
        seen += 1;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if strict {
        if seen % 4 == 1 {
            return None;
        }
        if padding > 0 && (padding > 2 || (seen + padding) % 4 != 0) {
            return None;
        }
    }
    Some(out)
}

/// C ABI base64 encoder over a byte pointer and length; `PHP_BASE64_NO_PADDING` in `flags` drops
/// the `=` padding. Returns a new `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn php_base64_encode_ex(str: *const u8, length: usize, flags: zend_long) -> *mut zend_string {
    let data = if length == 0 { &[][..] } else { std::slice::from_raw_parts(str, length) };
    string::new(&base64_encode(data, flags & PHP_BASE64_NO_PADDING as zend_long == 0))
}

/// C ABI base64 decoder; `strict` rejects non-alphabet characters and bad padding as PHP's does.
/// Returns a new `zend_string`, or null on invalid strict input.
#[no_mangle]
pub unsafe extern "C" fn php_base64_decode_ex(str: *const u8, length: usize, strict: bool) -> *mut zend_string {
    let data = if length == 0 { &[][..] } else { std::slice::from_raw_parts(str, length) };
    match base64_decode(data, strict) {
        Some(decoded) => string::new(&decoded),
        None => ptr::null_mut(),
    }
}

/// Base64-encodes a byte buffer with padding; returns a new `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn php_base64_encode(str: *const u8, length: usize) -> *mut zend_string {
    php_base64_encode_ex(str, length, 0)
}

/// Decodes a base64 byte buffer in non-strict mode; returns a new `zend_string` (null never occurs
/// in non-strict mode).
#[no_mangle]
pub unsafe extern "C" fn php_base64_decode(str: *const u8, length: usize) -> *mut zend_string {
    php_base64_decode_ex(str, length, false)
}

/// Base64-encodes the bytes of a `zend_string` with padding.
#[no_mangle]
pub unsafe extern "C" fn php_base64_encode_str(str: *const zend_string) -> *mut zend_string {
    php_base64_encode_ex((*str).val.as_ptr().cast(), (*str).len, 0)
}

/// Decodes the bytes of a `zend_string` in non-strict mode.
#[no_mangle]
pub unsafe extern "C" fn php_base64_decode_str(str: *const zend_string) -> *mut zend_string {
    php_base64_decode_ex((*str).val.as_ptr().cast(), (*str).len, false)
}

// ------------------------------------------------------------ math

/// Rounds the decimal `value` stands for to `places` digits.
pub fn round(value: f64, places: i32, mode: c_int) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let text = format!("{:e}", value.abs());
    let (mantissa, exponent) = text.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let digits: Vec<u8> = mantissa.bytes().filter(|b| *b != b'.').map(|b| b - b'0').collect();
    // value = 0.d1d2... * 10^(exponent + 1); keep `keep` leading digits.
    let keep = exponent + 1 + places;
    if keep < 0 {
        return 0.0f64.copysign(value);
    }
    let keep = keep as usize;
    if keep >= digits.len() {
        return value;
    }
    let mut kept: Vec<u8> = digits[..keep].to_vec();
    let rest = &digits[keep..];
    let first = rest[0];
    let beyond = rest[1..].iter().any(|&d| d != 0);
    let negative = value < 0.0;
    let last_odd = kept.last().map_or(false, |d| d % 2 == 1);
    let up = match first.cmp(&5) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal if beyond => true,
        std::cmp::Ordering::Equal => match mode {
            PHP_ROUND_HALF_DOWN => false,
            PHP_ROUND_HALF_EVEN => last_odd,
            PHP_ROUND_HALF_ODD => !last_odd,
            _ => true,
        },
    };
    let mut carry = up;
    for d in kept.iter_mut().rev() {
        if !carry {
            break;
        }
        if *d == 9 {
            *d = 0;
        } else {
            *d += 1;
            carry = false;
        }
    }
    let mut text = String::new();
    if carry {
        text.push('1');
    }
    for d in &kept {
        text.push((b'0' + d) as char);
    }
    if text.is_empty() {
        text.push('0');
    }
    let magnitude: f64 = format!("{text}e{}", exponent + 1 - keep as i32).parse().unwrap_or(0.0);
    if negative {
        -magnitude
    } else {
        magnitude
    }
}

/// C ABI entry for PHP's `round()` on a float: rounds `value` to `places` decimals using rounding
/// `mode`.
#[no_mangle]
pub extern "C" fn _php_math_round(value: f64, places: c_int, mode: c_int) -> f64 {
    round(value, places, mode)
}

/// Alias of `_php_math_round`; rounds `value` to `places` decimals with rounding `mode`.
#[no_mangle]
pub extern "C" fn php_math_round(value: f64, places: c_int, mode: c_int) -> f64 {
    round(value, places, mode)
}

/// Formats `d` like `number_format()`: rounds to `dec` places (half up), inserts the thousands
/// separator and decimal point (a NUL character omits either) and returns a new `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn _php_math_number_format(d: f64, dec: c_int, dec_point: c_char, thousand_sep: c_char) -> *mut zend_string {
    let dec = dec.max(0) as usize;
    let rounded = round(d, dec as i32, PHP_ROUND_HALF_UP);
    let text = format!("{:.*}", dec, rounded.abs());
    let (int_part, frac_part) = text.split_once('.').map_or((text.as_str(), ""), |(a, b)| (a, b));
    // Built as bytes: a separator is one raw byte (0xFF stays 0xFF), never a UTF-8 encoded char.
    let mut out = Vec::new();
    if rounded < 0.0 && rounded != 0.0 {
        out.push(b'-');
    }
    for (i, c) in int_part.bytes().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 && thousand_sep != 0 {
            out.push(thousand_sep as u8);
        }
        out.push(c);
    }
    if dec > 0 {
        if dec_point != 0 {
            out.push(dec_point as u8);
        }
        out.extend_from_slice(frac_part.as_bytes());
    }
    string::new(&out)
}

/// Converts an integer to its text in the given base (clamped to 2..36), lowercase digits, treating
/// the value as unsigned; returns a new `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn _php_math_longtobase(arg: zend_long, base: c_int) -> *mut zend_string {
    let mut value = arg as u64;
    let base = base.clamp(2, 36) as u64;
    let mut digits = Vec::new();
    loop {
        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % base) as usize]);
        value /= base;
        if value == 0 {
            break;
        }
    }
    digits.reverse();
    string::new(&digits)
}

/// Returns the IEEE NaN value.
#[no_mangle]
pub extern "C" fn php_get_nan() -> f64 {
    f64::NAN
}

/// Returns positive infinity.
#[no_mangle]
pub extern "C" fn php_get_inf() -> f64 {
    f64::INFINITY
}

/// `zend_dtoa`: the significant digits of `d` (malloc'd), the decimal point
/// position and the sign. Mode 0 is the shortest round trip, 2 `ndigits`
/// significant digits, 3 `ndigits` past the decimal point.
#[no_mangle]
pub unsafe extern "C" fn zend_dtoa(
    d: f64,
    mode: c_int,
    ndigits: c_int,
    decpt: *mut c_int,
    sign: *mut bool,
    rve: *mut *mut c_char,
) -> *mut c_char {
    *sign = d.is_sign_negative();
    let text: String;
    if !d.is_finite() {
        *decpt = 9999;
        text = if d.is_nan() { "NaN".into() } else { "Infinity".into() };
    } else if d == 0.0 {
        *decpt = 1;
        text = "0".into();
    } else {
        let formatted = match mode {
            2 | 4 => format!("{:.*e}", (ndigits.max(1) - 1) as usize, d.abs()),
            3 | 5 => {
                let fixed = format!("{:.*}", ndigits.max(0) as usize, d.abs());
                let digits: String = fixed.chars().filter(|c| *c != '.').collect();
                let int_len = fixed.split('.').next().map_or(0, str::len) as i32;
                let trimmed = digits.trim_start_matches('0');
                let leading = digits.len() - trimmed.len();
                let mut kept = trimmed.trim_end_matches('0').to_string();
                if kept.is_empty() {
                    kept.push('0');
                    *decpt = 1;
                } else {
                    *decpt = int_len - leading as i32;
                }
                let out = alloc(kept.len() + 1).cast::<c_char>();
                ptr::copy_nonoverlapping(kept.as_ptr().cast(), out, kept.len());
                *out.add(kept.len()) = 0;
                if !rve.is_null() {
                    *rve = out.add(kept.len());
                }
                return out;
            }
            _ => format!("{:e}", d.abs()),
        };
        let (mantissa, exponent) = formatted.split_once('e').unwrap();
        let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
        while digits.len() > 1 && digits.ends_with('0') {
            digits.pop();
        }
        *decpt = exponent.parse::<c_int>().unwrap() + 1;
        text = digits;
    }
    let out = alloc(text.len() + 1).cast::<c_char>();
    ptr::copy_nonoverlapping(text.as_ptr().cast(), out, text.len());
    *out.add(text.len()) = 0;
    if !rve.is_null() {
        *rve = out.add(text.len());
    }
    out
}

/// Frees a digit string previously returned by `zend_dtoa`.
#[no_mangle]
pub unsafe extern "C" fn zend_freedtoa(s: *mut c_char) {
    crate::alloc::free(s.cast());
}

// ------------------------------------------------------------ strings

/// Builds a byte slice from a C pointer and length, returning an empty slice for length 0 so a null
/// pointer is never dereferenced.
unsafe fn cbytes<'a>(s: *const c_char, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(s.cast(), len)
    }
}

/// Copies at most `siz - 1` bytes of `src` into `dst`, always NUL-terminating when `siz > 0`;
/// returns the length of `src` (BSD `strlcpy` semantics).
#[no_mangle]
pub unsafe extern "C" fn php_strlcpy(dst: *mut c_char, src: *const c_char, siz: usize) -> usize {
    let source = CStr::from_ptr(src).to_bytes();
    if siz > 0 {
        let n = source.len().min(siz - 1);
        ptr::copy_nonoverlapping(src, dst, n);
        *dst.add(n) = 0;
    }
    source.len()
}

/// Appends `src` to the NUL-terminated string in `dst` without exceeding `siz` bytes total; returns
/// the length it tried to create (BSD `strlcat` semantics).
#[no_mangle]
pub unsafe extern "C" fn php_strlcat(dst: *mut c_char, src: *const c_char, siz: usize) -> usize {
    let mut dlen = 0;
    while dlen < siz && *dst.add(dlen) != 0 {
        dlen += 1;
    }
    let source = CStr::from_ptr(src).to_bytes();
    if dlen == siz {
        return siz + source.len();
    }
    let n = source.len().min(siz - dlen - 1);
    ptr::copy_nonoverlapping(src, dst.add(dlen), n);
    *dst.add(dlen + n) = 0;
    dlen + source.len()
}

/// Uppercases `len` bytes of `s` in place (ASCII) and returns `s`.
#[no_mangle]
pub unsafe extern "C" fn php_strtoupper(s: *mut c_char, len: usize) -> *mut c_char {
    string::zend_str_toupper(s, len);
    s
}

/// Lowercases `len` bytes of `s` in place (ASCII) and returns `s`.
#[no_mangle]
pub unsafe extern "C" fn php_strtolower(s: *mut c_char, len: usize) -> *mut c_char {
    string::zend_str_tolower(s, len);
    s
}

/// Returns an ASCII-uppercased `zend_string`; the original with one more reference when nothing
/// changes (see `zend_string_toupper_ex`).
#[no_mangle]
pub unsafe extern "C" fn php_string_toupper(s: *mut zend_string) -> *mut zend_string {
    string::zend_string_toupper_ex(s, false)
}

/// Returns an ASCII-lowercased `zend_string`; the original with one more reference when nothing
/// changes (see `zend_string_tolower_ex`).
#[no_mangle]
pub unsafe extern "C" fn php_string_tolower(s: *mut zend_string) -> *mut zend_string {
    string::zend_string_tolower_ex(s, false)
}

/// Translates bytes of `str` in place: each byte found in `str_from` is replaced by the byte at the
/// same position in `str_to`; returns `str`.
#[no_mangle]
pub unsafe extern "C" fn php_strtr(str: *mut c_char, len: usize, str_from: *const c_char, str_to: *const c_char, trlen: usize) -> *mut c_char {
    let from = cbytes(str_from, trlen);
    let to = cbytes(str_to, trlen);
    for i in 0..len {
        let c = *str.add(i) as u8;
        if let Some(at) = from.iter().position(|&f| f == c) {
            *str.add(i) = to[at] as c_char;
        }
    }
    str
}

/// Escapes `'`, `"`, `\` with a backslash and NUL as `\0`, like `addslashes()`; returns a new
/// `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn php_addslashes(str: *mut zend_string) -> *mut zend_string {
    let mut out = Vec::with_capacity((*str).len);
    for &c in bytes(str) {
        match c {
            0 => out.extend_from_slice(b"\\0"),
            b'\'' | b'"' | b'\\' => {
                out.push(b'\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    string::new(&out)
}

/// Removes backslash escapes in place, like `stripslashes()` (`\0` becomes a NUL byte, `\\` a
/// single backslash) and updates the string's length.
#[no_mangle]
pub unsafe extern "C" fn php_stripslashes(str: *mut zend_string) {
    let input = bytes(str).to_vec();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'\\' {
            i += 1;
            if i < input.len() {
                out.push(if input[i] == b'0' { 0 } else { input[i] });
            }
        } else {
            out.push(input[i]);
        }
        i += 1;
    }
    ptr::copy_nonoverlapping(out.as_ptr(), (*str).val.as_mut_ptr().cast::<u8>(), out.len());
    (*str).len = out.len();
    *(*str).val.as_mut_ptr().add(out.len()) = 0;
}

/// Decodes C-style escapes in place (`\n \t \r \a \v \b \f`, `\xHH`, octal, any other escaped byte
/// as itself), like `stripcslashes()`, and updates the string's length.
#[no_mangle]
pub unsafe extern "C" fn php_stripcslashes(str: *mut zend_string) {
    let input = bytes(str).to_vec();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] != b'\\' || i + 1 >= input.len() {
            out.push(input[i]);
            i += 1;
            continue;
        }
        i += 1;
        let c = input[i];
        match c {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(0x07),
            b'v' => out.push(0x0b),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'x' if i + 1 < input.len() && input[i + 1].is_ascii_hexdigit() => {
                let mut value = 0u8;
                let mut n = 0;
                while n < 2 && i + 1 < input.len() && input[i + 1].is_ascii_hexdigit() {
                    i += 1;
                    value = value.wrapping_mul(16).wrapping_add((input[i] as char).to_digit(16).unwrap() as u8);
                    n += 1;
                }
                out.push(value);
            }
            b'0'..=b'7' => {
                let mut value = 0u32;
                let mut n = 0;
                while n < 3 && i < input.len() && (b'0'..=b'7').contains(&input[i]) {
                    value = value * 8 + (input[i] - b'0') as u32;
                    i += 1;
                    n += 1;
                }
                i -= 1;
                out.push(value as u8);
            }
            other => out.push(other),
        }
        i += 1;
    }
    ptr::copy_nonoverlapping(out.as_ptr(), (*str).val.as_mut_ptr().cast::<u8>(), out.len());
    (*str).len = out.len();
    *(*str).val.as_mut_ptr().add(out.len()) = 0;
}

/// The byte set a `what` list names, with `a..z` ranges.
fn char_mask(what: &[u8]) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut i = 0;
    while i < what.len() {
        if i + 3 < what.len() && what[i + 1] == b'.' && what[i + 2] == b'.' && what[i + 3] >= what[i] {
            for c in what[i]..=what[i + 3] {
                mask[c as usize] = true;
            }
            i += 4;
        } else {
            mask[what[i] as usize] = true;
            i += 1;
        }
    }
    mask
}

/// Backslash-escapes the bytes listed in `what` (with `a..z` ranges); non-printable bytes become C
/// escapes or three-digit octal, like `addcslashes()`. Returns a new `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn php_addcslashes_str(str: *const c_char, len: usize, what: *const c_char, what_len: usize) -> *mut zend_string {
    let mask = char_mask(cbytes(what, what_len));
    let mut out = Vec::with_capacity(len);
    for &c in cbytes(str, len) {
        if !mask[c as usize] {
            out.push(c);
            continue;
        }
        if !(32..=126).contains(&c) {
            out.push(b'\\');
            match c {
                b'\n' => out.push(b'n'),
                b'\t' => out.push(b't'),
                b'\r' => out.push(b'r'),
                0x07 => out.push(b'a'),
                0x0b => out.push(b'v'),
                0x08 => out.push(b'b'),
                0x0c => out.push(b'f'),
                _ => out.extend_from_slice(format!("{c:03o}").as_bytes()),
            }
            continue;
        }
        out.push(b'\\');
        out.push(c);
    }
    string::new(&out)
}

/// `zend_string` form of `php_addcslashes_str`.
#[no_mangle]
pub unsafe extern "C" fn php_addcslashes(str: *mut zend_string, what: *const c_char, what_len: usize) -> *mut zend_string {
    php_addcslashes_str((*str).val.as_ptr(), (*str).len, what, what_len)
}

/// Replaces every non-overlapping occurrence of `needle` in `haystack` with `str`, scanning left to
/// right; an empty needle returns a copy. Returns a new `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn php_str_to_str(
    haystack: *const c_char,
    length: usize,
    needle: *const c_char,
    needle_len: usize,
    str: *const c_char,
    str_len: usize,
) -> *mut zend_string {
    let hay = cbytes(haystack, length);
    let needle = cbytes(needle, needle_len);
    let replacement = cbytes(str, str_len);
    if needle.is_empty() {
        return string::new(hay);
    }
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        if hay[i..].starts_with(needle) {
            out.extend_from_slice(replacement);
            i += needle.len();
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    string::new(&out)
}

/// Trims bytes from a string per `mode` (bit 1 left, bit 2 right, 3 both) using the `what`
/// character list with `a..z` ranges (default whitespace and NUL when `what` is null); returns the
/// same string with a new reference when nothing was trimmed, otherwise a new string.
#[no_mangle]
pub unsafe extern "C" fn php_trim(str: *mut zend_string, what: *const c_char, what_len: usize, mode: c_int) -> *mut zend_string {
    let mask = if what.is_null() { char_mask(b" \n\r\t\x0b\0") } else { char_mask(cbytes(what, what_len)) };
    let text = bytes(str);
    let mut start = 0;
    let mut end = text.len();
    if mode & 1 != 0 {
        while start < end && mask[text[start] as usize] {
            start += 1;
        }
    }
    if mode & 2 != 0 {
        while end > start && mask[text[end - 1] as usize] {
            end -= 1;
        }
    }
    if start == 0 && end == text.len() {
        return string::zend_string_copy(str);
    }
    string::new(&text[start..end])
}

// ------------------------------------------------------------ randomness

extern "C" {
    /// libc `getentropy`: fills a buffer (at most 256 bytes per call) from the operating system's
    /// entropy source; returns 0 on success.
    fn getentropy(buf: *mut c_void, buflen: usize) -> c_int;
}

/// Fills `bytes` from the operating system's CSPRNG.
#[no_mangle]
pub unsafe extern "C" fn php_random_bytes(bytes: *mut c_void, size: usize, should_throw: bool) -> zend_result {
    let mut done = 0;
    while done < size {
        let chunk = (size - done).min(256);
        if getentropy(bytes.cast::<u8>().add(done).cast(), chunk) != 0 {
            if should_throw {
                crate::exception::throw(
                    crate::exception::zend_ce_exception,
                    b"Failed to retrieve randomness from the operating system",
                    0,
                );
            }
            return FAILURE;
        }
        done += chunk;
    }
    SUCCESS
}

/// Draws a uniformly distributed integer in `[min, max]` from OS entropy using rejection sampling,
/// storing it in `*result`. `min > max` yields `FAILURE` (and a `ValueError` when `should_throw`).
#[no_mangle]
pub unsafe extern "C" fn php_random_int(min: zend_long, max: zend_long, result: *mut zend_long, should_throw: bool) -> zend_result {
    if min > max {
        if should_throw {
            crate::exception::throw_error(crate::exception::zend_ce_value_error, "Minimum value must be less than or equal to the maximum value");
        }
        return FAILURE;
    }
    let range = (max as i128 - min as i128) as u128;
    if range == 0 {
        *result = min;
        return SUCCESS;
    }
    let bound = range + 1;
    let limit = (u64::MAX as u128 + 1) - ((u64::MAX as u128 + 1) % bound);
    loop {
        let mut raw = 0u64;
        if php_random_bytes((&mut raw as *mut u64).cast(), 8, should_throw) != SUCCESS {
            return FAILURE;
        }
        if (raw as u128) < limit {
            *result = (min as i128 + (raw as u128 % bound) as i128) as zend_long;
            return SUCCESS;
        }
    }
}

struct Mt {
    state: [u32; 624],
    index: usize,
    seeded: bool,
}

static MT: Global<Mt> = Global::new(Mt { state: [0; 624], index: 624, seeded: false });

/// Seeds the Mersenne Twister state with `seed` and marks it seeded.
#[no_mangle]
pub unsafe extern "C" fn php_mt_srand(seed: u32) {
    let mt = MT.get();
    mt.state[0] = seed;
    for i in 1..624 {
        mt.state[i] = 1812433253u32.wrapping_mul(mt.state[i - 1] ^ (mt.state[i - 1] >> 30)).wrapping_add(i as u32);
    }
    mt.index = 624;
    mt.seeded = true;
}

/// Returns the next 32-bit Mersenne Twister output, seeding from OS entropy first if never seeded.
#[no_mangle]
pub unsafe extern "C" fn php_mt_rand() -> u32 {
    let mt = MT.get();
    if !mt.seeded {
        let mut seed = 0u32;
        php_random_bytes((&mut seed as *mut u32).cast(), 4, false);
        php_mt_srand(seed);
    }
    let mt = MT.get();
    if mt.index >= 624 {
        for i in 0..624 {
            let y = (mt.state[i] & 0x80000000) | (mt.state[(i + 1) % 624] & 0x7fffffff);
            let mut next = mt.state[(i + 397) % 624] ^ (y >> 1);
            if y & 1 != 0 {
                next ^= 0x9908b0df;
            }
            mt.state[i] = next;
        }
        mt.index = 0;
    }
    let mut y = mt.state[mt.index];
    mt.index += 1;
    y ^= y >> 11;
    y ^= (y << 7) & 0x9d2c5680;
    y ^= (y << 15) & 0xefc60000;
    y ^ (y >> 18)
}

/// php-src `php_random_range32` over MT: one draw reduced into `[0, umax]` by masking a power-of-two
/// span or by rejection sampling (no modulo bias); `umax == u32::MAX` returns the raw draw.
unsafe fn mt_range32(umax: u32) -> u32 {
    let mut value = php_mt_rand();
    if umax == u32::MAX {
        return value;
    }
    let span = umax + 1;
    if span & (span - 1) == 0 {
        return value & (span - 1);
    }
    // Largest value under which every residue is equally likely.
    let limit = u32::MAX - (u32::MAX % span) - 1;
    while value > limit {
        value = php_mt_rand();
    }
    value % span
}

/// Two MT draws joined into one 64-bit value, the first draw as the low word, as php-src
/// accumulates a 64-bit result from a 32-bit engine.
unsafe fn mt_draw64() -> u64 {
    let low = php_mt_rand() as u64;
    low | (php_mt_rand() as u64) << 32
}

/// php-src `php_random_range64` over MT: a 64-bit draw reduced into `[0, umax]` like
/// `mt_range32`; `umax == u64::MAX` returns the raw 64-bit value.
unsafe fn mt_range64(umax: u64) -> u64 {
    let mut value = mt_draw64();
    if umax == u64::MAX {
        return value;
    }
    let span = umax + 1;
    if span & (span - 1) == 0 {
        return value & (span - 1);
    }
    let limit = u64::MAX - (u64::MAX % span) - 1;
    while value > limit {
        value = mt_draw64();
    }
    value % span
}

/// Returns an integer in `[min, max]` the way php-src's `php_mt_rand_range` does, so a seeded
/// sequence matches PHP's `mt_rand(min, max)`: spans that fit 32 bits take one draw, wider spans
/// take two, and both reject draws that would bias the result.
#[no_mangle]
pub unsafe extern "C" fn php_mt_rand_range(min: zend_long, max: zend_long) -> zend_long {
    let umax = (max as u64).wrapping_sub(min as u64);
    let value = if umax > u32::MAX as u64 {
        mt_range64(umax)
    } else {
        mt_range32(umax as u32) as u64
    };
    (min as u64).wrapping_add(value) as zend_long
}

/// Entry used for `mt_rand()` with a range; delegates to `php_mt_rand_range`.
#[no_mangle]
pub unsafe extern "C" fn php_mt_rand_common(min: zend_long, max: zend_long) -> zend_long {
    php_mt_rand_range(min, max)
}

/// Returns a 64-bit seed drawn from OS entropy (0 if that fails).
#[no_mangle]
pub unsafe extern "C" fn php_random_generate_fallback_seed() -> u64 {
    let mut seed = 0u64;
    php_random_bytes((&mut seed as *mut u64).cast(), 8, false);
    seed
}

// ------------------------------------------------------------ iconv

extern "C" {
    /// libc `iconv_open`: opens a conversion descriptor from `fromcode` to `tocode`; returns
    /// `(iconv_t)-1` on failure.
    fn iconv_open(tocode: *const c_char, fromcode: *const c_char) -> *mut c_void;
    /// libc `iconv`: converts bytes between the descriptor's charsets, advancing the in/out
    /// pointers and counts; returns `(size_t)-1` with `errno` set on error.
    fn iconv(cd: *mut c_void, inbuf: *mut *mut c_char, inleft: *mut usize, outbuf: *mut *mut c_char, outleft: *mut usize) -> usize;
    /// libc `iconv_close`: releases a conversion descriptor.
    fn iconv_close(cd: *mut c_void) -> c_int;
}

/// Reads the C `errno` of the calling thread on macOS via `__error()`.
#[cfg(target_os = "macos")]
unsafe fn errno() -> c_int {
    extern "C" {
        /// libc: returns the address of the calling thread's `errno` (macOS).
        fn __error() -> *mut c_int;
    }
    *__error()
}

/// Reads the C `errno` of the calling thread on non-macOS targets via `__errno_location()`.
#[cfg(not(target_os = "macos"))]
unsafe fn errno() -> c_int {
    extern "C" {
        /// libc: returns the address of the calling thread's `errno` (glibc/musl).
        fn __errno_location() -> *mut c_int;
    }
    *__errno_location()
}

const E2BIG: c_int = 7;
const EILSEQ_MACOS: c_int = 92;
const EILSEQ_LINUX: c_int = 84;
const EINVAL: c_int = 22;

/// `php_iconv_string`: converts `in_len` bytes from `in_charset` to
/// `out_charset` with the system iconv, as ext/iconv does. `*out` receives
/// what was converted, even on failure.
#[no_mangle]
pub unsafe extern "C" fn php_iconv_string(
    in_p: *const c_char,
    in_len: usize,
    out: *mut *mut zend_string,
    out_charset: *const c_char,
    in_charset: *const c_char,
) -> php_iconv_err_t {
    *out = ptr::null_mut();
    let cd = iconv_open(out_charset, in_charset);
    if cd as isize == -1 {
        return if errno() == EINVAL { PHP_ICONV_ERR_WRONG_CHARSET } else { PHP_ICONV_ERR_CONVERTER };
    }
    let mut converted: Vec<u8> = Vec::with_capacity(in_len + 16);
    let mut in_ptr = in_p as *mut c_char;
    let mut in_left = in_len;
    let mut chunk = [0u8; 4096];
    let mut result = PHP_ICONV_ERR_SUCCESS;
    loop {
        let mut out_ptr = chunk.as_mut_ptr().cast::<c_char>();
        let mut out_left = chunk.len();
        let flushing = in_left == 0;
        let status = if flushing {
            iconv(cd, ptr::null_mut(), ptr::null_mut(), &mut out_ptr, &mut out_left)
        } else {
            iconv(cd, &mut in_ptr, &mut in_left, &mut out_ptr, &mut out_left)
        };
        converted.extend_from_slice(&chunk[..chunk.len() - out_left]);
        if status == usize::MAX {
            match errno() {
                E2BIG => continue,
                EILSEQ_MACOS | EILSEQ_LINUX => {
                    result = PHP_ICONV_ERR_ILLEGAL_SEQ;
                    break;
                }
                EINVAL => {
                    result = PHP_ICONV_ERR_ILLEGAL_CHAR;
                    break;
                }
                _ => {
                    result = PHP_ICONV_ERR_UNKNOWN;
                    break;
                }
            }
        }
        if flushing {
            break;
        }
    }
    iconv_close(cd);
    *out = string::new(&converted);
    result
}

// ------------------------------------------------------------ files

extern "C" {
    /// libc `mkstemp`: creates and opens a unique file from a template ending in `XXXXXX`,
    /// rewriting the template; returns the descriptor or -1.
    fn mkstemp(template: *mut c_char) -> c_int;
    /// libc `fdopen`: wraps an open file descriptor in a `FILE*` with the given mode.
    fn fdopen(fd: c_int, mode: *const c_char) -> *mut FILE;
    /// libc `close`: closes a file descriptor; returns 0 or -1.
    fn close(fd: c_int) -> c_int;
    /// libc `scandir`: reads a directory into a malloc'd array of entries, optionally filtered and
    /// sorted; returns the entry count or -1.
    fn scandir(
        dirname: *const c_char,
        namelist: *mut *mut *mut dirent,
        select: Option<unsafe extern "C" fn(*const dirent) -> c_int>,
        compar: Option<unsafe extern "C" fn(*mut *const dirent, *mut *const dirent) -> c_int>,
    ) -> c_int;
    /// libc `alphasort`: `scandir` comparator ordering directory entries by name.
    fn alphasort(a: *mut *const dirent, b: *mut *const dirent) -> c_int;
}

/// No `open_basedir` is configured: every path is allowed.
#[no_mangle]
pub unsafe extern "C" fn php_check_open_basedir(_path: *const c_char) -> c_int {
    0
}

/// `open_basedir` check with a warning flag; no restriction exists, so it always allows the path
/// (returns 0).
#[no_mangle]
pub unsafe extern "C" fn php_check_open_basedir_ex(_path: *const c_char, _warn: c_int) -> c_int {
    0
}

/// Returns the temp directory (`$TMPDIR` without a trailing slash, else `/tmp`) as a C string
/// cached for the process lifetime.
#[no_mangle]
pub unsafe extern "C" fn php_get_temporary_directory() -> *const c_char {
    static DIRECTORY: Global<Option<std::ffi::CString>> = Global::new(None);
    let slot = DIRECTORY.get();
    if slot.is_none() {
        let dir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        let dir = dir.trim_end_matches('/').to_string();
        *slot = Some(std::ffi::CString::new(if dir.is_empty() { "/".to_string() } else { dir }).unwrap());
    }
    slot.as_ref().unwrap().as_ptr()
}

/// Creates a unique temp file `<dir>/<prefix>XXXXXX` with `mkstemp` (default dir and prefix when
/// null), stores its path in `*opened_path_p` if requested, and returns the descriptor or -1.
#[no_mangle]
pub unsafe extern "C" fn php_open_temporary_fd_ex(
    dir: *const c_char,
    pfx: *const c_char,
    opened_path_p: *mut *mut zend_string,
    _flags: u32,
) -> c_int {
    let dir = if dir.is_null() || *dir == 0 { php_get_temporary_directory() } else { dir };
    let prefix = if pfx.is_null() { "tmp".into() } else { CStr::from_ptr(pfx).to_string_lossy().into_owned() };
    let template = format!("{}/{}XXXXXX", CStr::from_ptr(dir).to_string_lossy(), prefix);
    let mut buffer = template.into_bytes();
    buffer.push(0);
    let fd = mkstemp(buffer.as_mut_ptr().cast());
    if fd >= 0 && !opened_path_p.is_null() {
        *opened_path_p = string::new(&buffer[..buffer.len() - 1]);
    }
    fd
}

/// `php_open_temporary_fd_ex` with no flags.
#[no_mangle]
pub unsafe extern "C" fn php_open_temporary_fd(dir: *const c_char, pfx: *const c_char, opened_path_p: *mut *mut zend_string) -> c_int {
    php_open_temporary_fd_ex(dir, pfx, opened_path_p, 0)
}

/// Creates a temp file as in `php_open_temporary_fd` and returns it as a `FILE*` opened `r+b`, or
/// null on failure.
#[no_mangle]
pub unsafe extern "C" fn php_open_temporary_file(dir: *const c_char, pfx: *const c_char, opened_path_p: *mut *mut zend_string) -> *mut FILE {
    let fd = php_open_temporary_fd(dir, pfx, opened_path_p);
    if fd < 0 {
        return ptr::null_mut();
    }
    let fp = fdopen(fd, c"r+b".as_ptr());
    if fp.is_null() {
        // As php-src: the descriptor is ours until a FILE* owns it.
        close(fd);
    }
    fp
}

/// Directory listing through libc `scandir` with the caller's selector and comparator.
#[no_mangle]
pub unsafe extern "C" fn php_scandir(
    dirname: *const c_char,
    namelist: *mut *mut *mut dirent,
    selector: Option<unsafe extern "C" fn(*const dirent) -> c_int>,
    compare: Option<unsafe extern "C" fn(*mut *const dirent, *mut *const dirent) -> c_int>,
) -> c_int {
    scandir(dirname, namelist, selector, compare)
}

/// Name-ordering comparator for `php_scandir`, delegating to libc `alphasort`.
#[no_mangle]
pub unsafe extern "C" fn php_alphasort(a: *mut *const dirent, b: *mut *const dirent) -> c_int {
    alphasort(a, b)
}

// ------------------------------------------------------------ the incomplete class

/// Returns a new reference to the original class name an incomplete-class object remembers
/// (`__PHP_Incomplete_Class_Name`), or null if it has none.
#[no_mangle]
pub unsafe extern "C" fn php_lookup_class_name(object: *mut zend_object) -> *mut zend_string {
    let name = crate::object::find_property(object, b"__PHP_Incomplete_Class_Name");
    if name.is_null() || type_of(name) != IS_STRING {
        return ptr::null_mut();
    }
    string::zend_string_copy((*name).value.str)
}

/// Records the original class name on an incomplete-class object as its
/// `__PHP_Incomplete_Class_Name` property.
#[no_mangle]
pub unsafe extern "C" fn php_store_class_name(object: *mut zval, name: *mut zend_string) {
    let mut value = zval::UNDEF;
    set_str_copy(&mut value, name);
    crate::object::set_property((*object).value.obj, b"__PHP_Incomplete_Class_Name", &mut value);
}

/// Creates an instance of the incomplete-class entry through the standard object allocator.
#[no_mangle]
pub unsafe extern "C" fn php_create_incomplete_object(class_type: *mut zend_class_entry) -> *mut zend_object {
    crate::object::zend_objects_new(class_type)
}

/// No-op: there are no special object handlers to register for `__PHP_Incomplete_Class`.
#[no_mangle]
pub unsafe extern "C" fn php_register_incomplete_class_handlers() {}

// ------------------------------------------------------------ SAPI, variables, phpinfo()

/// Returns the current wall-clock time in seconds as a float, standing in for the request start
/// time.
#[no_mangle]
pub unsafe extern "C" fn sapi_get_request_time() -> f64 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    now.as_secs_f64()
}

/// Headers are dropped, as the CLI SAPI drops them.
#[no_mangle]
pub unsafe extern "C" fn sapi_add_header_ex(_header_line: *const c_char, _len: usize, _duplicate: bool, _replace: bool) -> c_int {
    SUCCESS
}

/// Dropped: there are no request variable arrays to populate; does nothing.
#[no_mangle]
pub unsafe extern "C" fn php_register_variable(_var: *const c_char, _val: *const c_char, _track_vars_array: *mut zval) {}

/// Dropped: there are no request variable arrays to populate; does nothing.
#[no_mangle]
pub unsafe extern "C" fn php_register_variable_ex(_var: *const c_char, _val: *mut zval, _track_vars_array: *mut zval) {}

/// Dropped: there are no request variable arrays to populate; does nothing.
#[no_mangle]
pub unsafe extern "C" fn php_register_variable_safe(_var: *const c_char, _val: *const c_char, _len: usize, _track_vars_array: *mut zval) {}

/// No-op: `phpinfo()` output is not produced by a compiled program.
#[no_mangle]
pub extern "C" fn php_info_print_table_start() {}

/// No-op: `phpinfo()` output is not produced by a compiled program.
#[no_mangle]
pub extern "C" fn php_info_print_table_end() {}

/// No-op: `phpinfo()` output is not produced by a compiled program.
#[no_mangle]
pub extern "C" fn php_info_print_table_colspan_header(_num_cols: c_int, _header: *const c_char) {}

/// No-op: `phpinfo()` output is not produced by a compiled program.
#[no_mangle]
pub extern "C" fn php_info_print_box_start(_bg: c_int) {}

/// No-op: `phpinfo()` output is not produced by a compiled program.
#[no_mangle]
pub extern "C" fn php_info_print_box_end() {}

/// No-op: `phpinfo()` output is not produced by a compiled program.
#[no_mangle]
pub extern "C" fn php_info_print_hr() {}

/// No-op: `phpinfo()` output is not produced by a compiled program.
#[no_mangle]
pub extern "C" fn php_info_print_module(_module: *mut zend_module_entry) {}

/// Returns a copy of the C string as a `zend_string`; no HTML escaping is applied since `phpinfo()`
/// output is never rendered.
#[no_mangle]
pub unsafe extern "C" fn php_info_html_esc(s: *const c_char) -> *mut zend_string {
    string::new(CStr::from_ptr(s).to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: `round` matches PHP for half-up, half-even and negative-places cases, including values
    /// such as 0.285 and 1.955 that binary floats misrepresent, and rounds tiny values to zero.
    #[test]
    fn rounds_decimals_like_php() {
        assert_eq!(round(0.285, 2, PHP_ROUND_HALF_UP), 0.29);
        assert_eq!(round(1.955, 2, PHP_ROUND_HALF_UP), 1.96);
        assert_eq!(round(-2.5, 0, PHP_ROUND_HALF_UP), -3.0);
        assert_eq!(round(2.5, 0, PHP_ROUND_HALF_EVEN), 2.0);
        assert_eq!(round(3.5, 0, PHP_ROUND_HALF_EVEN), 4.0);
        assert_eq!(round(1234.5678, -2, PHP_ROUND_HALF_UP), 1200.0);
        assert_eq!(round(9.99, 1, PHP_ROUND_HALF_UP), 10.0);
        assert_eq!(round(0.0004, 2, PHP_ROUND_HALF_UP), 0.0);
    }

    /// Test: base64 encodes with and without padding, decodes strictly, rejects a bad character in
    /// strict mode, and skips it in non-strict mode.
    #[test]
    fn base64_round_trips() {
        assert_eq!(base64_encode(b"foob", true), b"Zm9vYg==");
        assert_eq!(base64_encode(b"foob", false), b"Zm9vYg");
        assert_eq!(base64_decode(b"Zm9vYg==", true).unwrap(), b"foob");
        assert!(base64_decode(b"Zm9v!g==", true).is_none());
        assert_eq!(base64_decode(b"Zm9v!Yg==", false).unwrap(), b"foob");
    }

    /// Test: a seeded `php_mt_rand_range` sequence matches PHP 8.5's
    /// `mt_srand(42); mt_rand(1,100), mt_rand(1,100), mt_rand(PHP_INT_MIN,PHP_INT_MAX),
    /// mt_rand(0,0x1FFFFFFFF), mt_rand(5,5), mt_rand(1,100)`, covering the 32-bit path, the full
    /// 64-bit range (raw value, not `min`), a wide rejection-sampled span and a one-value span
    /// that still consumes a draw.
    #[test]
    fn mt_rand_range_matches_php_sequence() {
        unsafe {
            php_mt_srand(42);
            assert_eq!(php_mt_rand_range(1, 100), 43);
            assert_eq!(php_mt_rand_range(1, 100), 68);
            assert_eq!(php_mt_rand_range(zend_long::MIN, zend_long::MAX), -5839597450370612388);
            assert_eq!(php_mt_rand_range(0, 0x1_FFFF_FFFF), 7438857322);
            assert_eq!(php_mt_rand_range(5, 5), 5);
            assert_eq!(php_mt_rand_range(1, 100), 25);
        }
    }

    /// Test: `number_format` separators are emitted as raw bytes, so a 0xFF thousands separator and
    /// a 0xFE decimal point stay single bytes instead of turning into two-byte UTF-8 sequences.
    #[test]
    fn number_format_keeps_high_separator_bytes() {
        unsafe {
            let formatted = _php_math_number_format(1234567.891, 2, 0xFEu8 as c_char, 0xFFu8 as c_char);
            assert_eq!(bytes(formatted), b"1\xff234\xff567\xfe89");
        }
    }
}
