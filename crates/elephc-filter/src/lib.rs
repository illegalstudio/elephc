//! Purpose:
//! Implements PHP's `ext/filter` validation and sanitization rules behind a panic-free C ABI.
//!
//! Called from:
//! - The synthetic AST filter prelude through `extern "elephc_filter"` declarations.
//!
//! Key details:
//! - Inputs and outputs are binary-safe: strings enter as pointer/length and leave as a
//!   thread-local byte buffer rendered as hex for the caller to `hex2bin()`.
//! - Each entry point stays inside the supported ARM64/System V eight-integer-argument limit;
//!   option presence is encoded in `range_flags` rather than separate set booleans.
//! - Every entry point catches panics so no Rust unwind ever crosses into generated code.

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

mod regex;
mod sanitize;
mod strip_tags;
mod validate;

/// One successful filter outcome; `Failed` is PHP's `false`/`null` sentinel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    Failed,
    Null,
    Str(Vec<u8>),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl Default for Outcome {
    /// Represents "no call has run yet".
    fn default() -> Self { Self::Failed }
}

thread_local! {
    static RESULT: RefCell<Outcome> = const { RefCell::new(Outcome::Failed) };
    static HEX: RefCell<CString> = RefCell::new(CString::default());
}

/// Runs one bridge call and stores its outcome for the getter calls.
fn run(call: impl FnOnce() -> Outcome) -> i64 {
    let outcome = catch_unwind(AssertUnwindSafe(call)).unwrap_or(Outcome::Failed);
    let kind = match &outcome {
        Outcome::Failed => 0,
        Outcome::Str(_) => 1,
        Outcome::Int(_) => 2,
        Outcome::Float(_) => 3,
        Outcome::Bool(_) => 4,
        Outcome::Null => 5,
    };
    RESULT.with(|slot| *slot.borrow_mut() = outcome);
    kind
}

/// Decodes one lowercase/uppercase hex C string into bytes, rejecting invalid input as empty.
unsafe fn hex_arg(ptr: *const c_char) -> Vec<u8> {
    if ptr.is_null() { return Vec::new(); }
    let text = CStr::from_ptr(ptr).to_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut index = 0usize;
    while index + 1 < text.len() {
        let high = hex_digit(text[index]);
        let low = hex_digit(text[index + 1]);
        match (high, low) {
            (Some(high), Some(low)) => out.push((high << 4) | low),
            _ => return Vec::new(),
        }
        index += 2;
    }
    out
}

/// Converts one ASCII hex digit to its value.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Returns FILTER_VALIDATE_INT's result after applying `range_flags` bounds.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_int(
    flags: i64,
    value_hex: *const c_char,
    min: i64,
    max: i64,
    range_flags: i64,
) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        let Some(parsed) = validate::int(&value, flags) else {
            return Outcome::Failed;
        };
        if range_flags & 1 != 0 && parsed < min { return Outcome::Failed; }
        if range_flags & 2 != 0 && parsed > max { return Outcome::Failed; }
        Outcome::Int(parsed)
    })
}

/// Returns FILTER_VALIDATE_BOOL's result.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_bool(value_hex: *const c_char) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        match validate::boolean(&value) {
            Some(value) => Outcome::Bool(value),
            None => Outcome::Failed,
        }
    })
}

/// Returns FILTER_VALIDATE_FLOAT's result after separators and `range_flags` bounds.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn elephc_filter_apply_float(
    flags: i64,
    value_hex: *const c_char,
    decimal_hex: *const c_char,
    thousand_hex: *const c_char,
    range_flags: i64,
    min: f64,
    max: f64,
) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        let decimal_bytes = hex_arg(decimal_hex);
        let thousand = hex_arg(thousand_hex);
        let decimal = decimal_bytes.first().copied();
        let Some(parsed) = validate::float(
            &value,
            flags,
            decimal,
            (!thousand.is_empty()).then_some(thousand.as_slice()),
        ) else {
            return Outcome::Failed;
        };
        if range_flags & 1 != 0 && parsed < min { return Outcome::Failed; }
        if range_flags & 2 != 0 && parsed > max { return Outcome::Failed; }
        Outcome::Float(parsed)
    })
}

/// Returns FILTER_VALIDATE_DOMAIN's result.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_domain(flags: i64, value_hex: *const c_char) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        Outcome::Bool(validate::domain(&value, flags))
    })
}

/// Returns FILTER_VALIDATE_IP's result.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_ip(flags: i64, value_hex: *const c_char) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        Outcome::Bool(validate::ip(&value, flags))
    })
}

/// Returns FILTER_VALIDATE_MAC's result.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_mac(
    _flags: i64,
    value_hex: *const c_char,
    separator_hex: *const c_char,
) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        let separator = hex_arg(separator_hex);
        Outcome::Bool(validate::mac(&value, !separator.is_empty(), &separator))
    })
}

/// Returns one sanitizer's result.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_sanitize(
    filter: i64,
    flags: i64,
    value_hex: *const c_char,
) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        sanitize::apply(filter, flags, &value)
    })
}

/// Returns FILTER_VALIDATE_EMAIL's result.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_email(flags: i64, value_hex: *const c_char) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        Outcome::Bool(regex::email(&value, flags & 0x0010_0000 != 0))
    })
}

/// Returns FILTER_VALIDATE_REGEXP's result for one PHP-style pattern literal.
#[no_mangle]
pub unsafe extern "C" fn elephc_filter_apply_regexp(
    value_hex: *const c_char,
    pattern_hex: *const c_char,
) -> i64 {
    run(|| {
        let value = hex_arg(value_hex);
        let pattern = hex_arg(pattern_hex);
        Outcome::Bool(regex::php_pattern(&value, &pattern))
    })
}

/// Returns the kind of the last outcome: 0 fail, 1 string, 2 int, 3 float, 4 bool, 5 null.
#[no_mangle]
pub extern "C" fn elephc_filter_result_kind() -> i64 {
    RESULT.with(|slot| match &*slot.borrow() {
        Outcome::Failed => 0,
        Outcome::Str(_) => 1,
        Outcome::Int(_) => 2,
        Outcome::Float(_) => 3,
        Outcome::Bool(_) => 4,
        Outcome::Null => 5,
    })
}

/// Returns the last integer outcome, or zero for every other kind.
#[no_mangle]
pub extern "C" fn elephc_filter_result_int() -> i64 {
    RESULT.with(|slot| match &*slot.borrow() { Outcome::Int(value) => *value, _ => 0 })
}

/// Returns the last float outcome, or zero for every other kind.
#[no_mangle]
pub extern "C" fn elephc_filter_result_float() -> f64 {
    RESULT.with(|slot| match &*slot.borrow() { Outcome::Float(value) => *value, _ => 0.0 })
}

/// Returns the last boolean outcome as 1/0, or zero for every other kind.
#[no_mangle]
pub extern "C" fn elephc_filter_result_bool() -> i64 {
    RESULT.with(|slot| match &*slot.borrow() { Outcome::Bool(value) => i64::from(*value), _ => 0 })
}

/// Returns the last string outcome as lowercase hex, or an empty string.
///
/// Hex keeps the C string NUL-free, so `filter_var("\0", FILTER_UNSAFE_RAW)` survives the
/// ABI boundary before the PHP wrapper calls `hex2bin()`.
#[no_mangle]
pub extern "C" fn elephc_filter_result_hex() -> *const c_char {
    HEX.with(|slot| {
        let rendered = RESULT.with(|result| match &*result.borrow() {
            Outcome::Str(value) => value.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            _ => String::new(),
        });
        *slot.borrow_mut() = CString::new(rendered).unwrap_or_default();
        slot.borrow().as_ptr()
    })
}

#[cfg(test)]
mod tests;
