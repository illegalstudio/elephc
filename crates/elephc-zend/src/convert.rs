//! Purpose:
//! PHP's value conversions and comparisons: zval to int, float, string and
//! bool, numeric strings, float formatting, `==`/`===`/`<=>`, arithmetic on
//! zvals, and type names.
//!
//! Called from:
//! - Extension code (`zval_get_long`, `convert_to_string`, `zend_is_true`,
//!   `is_numeric_string`, `compare_function`...), and argument parsing.
//!
//! Key details:
//! - Semantics are PHP 8.5's in weak mode: leading-numeric strings convert
//!   with their numeric prefix, floats outside the integer range convert to
//!   0, `(string)$float` uses `precision` (14) significant digits and
//!   `serialize()`/`var_export()` the shortest round-trip form.
//! - Float digits come from Rust's correctly rounded formatting, then are
//!   laid out by PHP's `php_gcvt` rules (exponent past `precision` digits or
//!   below 1e-4; at least one fractional digit in exponent form).

use std::cmp::Ordering;
use std::ffi::{c_char, c_int, CStr};

use crate::abi::*;
use crate::string::{self, bytes};
use crate::zval::*;

// ------------------------------------------------------------ float formatting

/// The significant digits of `|value|` and the decimal point position
/// (`0.d1d2... * 10^decpt`), as `zend_dtoa` mode 2 (`precision` digits) or
/// mode 0 (shortest round trip, `precision <= 0`).
fn float_digits(value: f64, precision: i32) -> (String, i32) {
    let abs = value.abs();
    if abs == 0.0 {
        return ("0".to_string(), 1);
    }
    let text = if precision > 0 { format!("{:.*e}", (precision - 1) as usize, abs) } else { format!("{:e}", abs) };
    let (mantissa, exponent) = text.split_once('e').expect("exponent form");
    let exponent: i32 = exponent.parse().expect("exponent");
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    (digits, exponent + 1)
}

/// `php_gcvt`: `value` with `precision` significant digits (or the shortest
/// round trip when `precision <= 0`, switching to exponent form past 17).
pub fn gcvt(value: f64, precision: i32, dec_point: char, exp_char: char) -> String {
    if value.is_nan() {
        return "NAN".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-INF" } else { "INF" }.to_string();
    }
    let ndigit = if precision <= 0 { 17 } else { precision };
    let (digits, decpt) = float_digits(value, precision);
    let mut out = String::new();
    if value.is_sign_negative() {
        out.push('-');
    }
    let digits = digits.as_bytes();
    if if decpt < 0 { decpt < -3 } else { decpt > ndigit } {
        let exponent = decpt - 1;
        out.push(digits[0] as char);
        out.push(dec_point);
        if digits.len() == 1 {
            out.push('0');
        } else {
            out.push_str(std::str::from_utf8(&digits[1..]).unwrap());
        }
        out.push(exp_char);
        out.push(if exponent < 0 { '-' } else { '+' });
        out.push_str(&exponent.abs().to_string());
    } else if decpt <= 0 {
        out.push('0');
        out.push(dec_point);
        for _ in decpt..0 {
            out.push('0');
        }
        out.push_str(std::str::from_utf8(digits).unwrap());
    } else {
        let decpt = decpt as usize;
        for i in 0..decpt {
            out.push(if i < digits.len() { digits[i] as char } else { '0' });
        }
        if digits.len() > decpt {
            out.push(dec_point);
            out.push_str(std::str::from_utf8(&digits[decpt..]).unwrap());
        }
    }
    out
}

/// `(string)$float`.
pub fn double_to_php(value: f64, precision: i32) -> String {
    gcvt(value, precision, '.', 'E')
}

/// `var_export()`/`json`-style float text: shortest round trip, with `.0`
/// added to integral values when `zero_fraction`.
pub fn double_repr(value: f64, precision: i32, zero_fraction: bool) -> String {
    let mut text = gcvt(value, precision, '.', 'E');
    if zero_fraction && value.is_finite() && !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    text
}

/// `zend_gcvt`: formats `value` with `ndigit` significant digits in PHP's gcvt style (decimal point
/// and exponent marker as given) into `buf`, NUL-terminated, and returns `buf`. The caller's buffer
/// must be large enough.
#[no_mangle]
pub unsafe extern "C" fn zend_gcvt(value: f64, ndigit: c_int, dec_point: c_char, dec_point_rep: c_char, buf: *mut c_char) -> *mut c_char {
    let text = gcvt(value, ndigit.max(1), dec_point as u8 as char, dec_point_rep as u8 as char);
    std::ptr::copy_nonoverlapping(text.as_ptr().cast::<c_char>(), buf, text.len());
    *buf.add(text.len()) = 0;
    buf
}

/// `php_gcvt`: like `zend_gcvt`, but a precision of 0 or less (`serialize_precision`
/// = -1) asks for the shortest round-trip form, exponent past 17 digits.
#[no_mangle]
pub unsafe extern "C" fn php_gcvt(value: f64, precision: c_int, dec_point: c_char, dec_point_rep: c_char, buf: *mut c_char) -> *mut c_char {
    let text = gcvt(value, precision, dec_point as u8 as char, dec_point_rep as u8 as char);
    std::ptr::copy_nonoverlapping(text.as_ptr().cast::<c_char>(), buf, text.len());
    *buf.add(text.len()) = 0;
    buf
}

/// `zend_strtod`: parses a leading decimal floating-point number, storing the end position in `se`
/// when it is non-NULL. As php-src's (David Gay's) parser and unlike the C `strtod`, it reads only
/// `[+-]digits[.digits][e[+-]digits]`, whatever the locale: no hexadecimal, `inf` or `nan`, and
/// no leading whitespace. Without a digit it returns 0 with `se` at the start.
#[no_mangle]
pub unsafe extern "C" fn zend_strtod(s00: *const c_char, se: *mut *const c_char) -> f64 {
    let (value, len) = decimal_prefix(CStr::from_ptr(s00).to_bytes());
    if !se.is_null() {
        *se = s00.add(len);
    }
    value
}

/// The value and length of the decimal number `zend_strtod` reads at the start of `bytes`
/// (`(0.0, 0)` when there is none). Rust's parser rounds correctly, as Gay's does.
fn decimal_prefix(bytes: &[u8]) -> (f64, usize) {
    let digits_from = |mut at: usize| {
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        at
    };
    let start = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut end = digits_from(start);
    let mut digits = end - start;
    if bytes.get(end) == Some(&b'.') {
        let fraction_end = digits_from(end + 1);
        digits += fraction_end - (end + 1);
        if digits > 0 {
            end = fraction_end;
        }
    }
    if digits == 0 {
        return (0.0, 0);
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent_end = digits_from(end + 1 + sign);
        if exponent_end > end + 1 + sign {
            end = exponent_end;
        }
    }
    let text = std::str::from_utf8(&bytes[..end]).unwrap_or("0");
    (text.parse().unwrap_or(0.0), end)
}

/// Parses digits of the given radix into a double from `p` on; `endptr`
/// receives the first unparsed byte, or `str` (the caller's start) when no
/// digit was read.
unsafe fn radix_strtod(str: *const c_char, mut p: *const c_char, endptr: *mut *const c_char, radix: u32) -> f64 {
    let mut value = 0.0f64;
    let mut any = false;
    while let Some(digit) = (*p as u8 as char).to_digit(radix) {
        value = value * radix as f64 + digit as f64;
        any = true;
        p = p.add(1);
    }
    if !endptr.is_null() {
        *endptr = if any { p } else { str };
    }
    value
}

/// Skips a `0x`/`0b`-style prefix (either case of `letter`) at `str`, as
/// `zend_hex_strtod` and `zend_bin_strtod` do; reads at most two bytes and
/// stops at the terminator.
unsafe fn skip_prefix(str: *const c_char, letter: u8) -> *const c_char {
    if *str as u8 == b'0' && (*str.add(1) as u8 | 0x20) == letter {
        str.add(2)
    } else {
        str
    }
}

/// `zend_hex_strtod`: parses hexadecimal digits into a double, after an
/// optional `0x`/`0X` prefix, as php-src does.
#[no_mangle]
pub unsafe extern "C" fn zend_hex_strtod(str: *const c_char, endptr: *mut *const c_char) -> f64 {
    radix_strtod(str, skip_prefix(str, b'x'), endptr, 16)
}

/// `zend_oct_strtod`: parses octal digits into a double. Like php-src it
/// skips the first byte (the literal's leading zero) unconditionally.
#[no_mangle]
pub unsafe extern "C" fn zend_oct_strtod(str: *const c_char, endptr: *mut *const c_char) -> f64 {
    if *str == 0 {
        if !endptr.is_null() {
            *endptr = str;
        }
        return 0.0;
    }
    radix_strtod(str, str.add(1), endptr, 8)
}

/// `zend_bin_strtod`: parses binary digits into a double, after an optional
/// `0b`/`0B` prefix, as php-src does.
#[no_mangle]
pub unsafe extern "C" fn zend_bin_strtod(str: *const c_char, endptr: *mut *const c_char) -> f64 {
    radix_strtod(str, skip_prefix(str, b'b'), endptr, 2)
}

// ------------------------------------------------------------ numeric strings

/// The result of reading a numeric string.
pub enum Numeric {
    Long(zend_long),
    Double(f64),
}

/// Whether a byte is PHP numeric-string whitespace (space, tab, newline, CR, VT, FF).
fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// PHP 8's numeric string rule. Returns the number, whether the integer
/// overflowed into a float (its sign), and whether non-whitespace trailing
/// data follows the number (accepted only when `allow_errors`).
pub fn parse_numeric(text: &[u8], allow_errors: bool) -> Option<(Numeric, i32, bool)> {
    let mut i = 0;
    while i < text.len() && is_ws(text[i]) {
        i += 1;
    }
    let start = i;
    if i < text.len() && (text[i] == b'-' || text[i] == b'+') {
        i += 1;
    }
    let int_start = i;
    while i < text.len() && text[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut is_double = false;
    let mut frac_digits = 0;
    if i < text.len() && text[i] == b'.' {
        let mut j = i + 1;
        while j < text.len() && text[j].is_ascii_digit() {
            j += 1;
        }
        frac_digits = j - i - 1;
        if int_digits + frac_digits > 0 {
            is_double = true;
            i = j;
        }
    }
    if int_digits + frac_digits == 0 {
        return None;
    }
    if i < text.len() && (text[i] == b'e' || text[i] == b'E') {
        let mut j = i + 1;
        if j < text.len() && (text[j] == b'-' || text[j] == b'+') {
            j += 1;
        }
        if j < text.len() && text[j].is_ascii_digit() {
            while j < text.len() && text[j].is_ascii_digit() {
                j += 1;
            }
            is_double = true;
            i = j;
        }
    }
    let number = &text[start..i];
    let mut end = i;
    while end < text.len() && is_ws(text[end]) {
        end += 1;
    }
    let trailing = end < text.len();
    if trailing && !allow_errors {
        return None;
    }
    let number = std::str::from_utf8(number).ok()?;
    if !is_double {
        if let Ok(value) = number.parse::<i64>() {
            return Some((Numeric::Long(value), 0, trailing));
        }
        let value: f64 = number.parse().ok()?;
        let overflow = if value < 0.0 { -1 } else { 1 };
        return Some((Numeric::Double(value), overflow, trailing));
    }
    let value: f64 = number.parse().ok()?;
    Some((Numeric::Double(value), 0, trailing))
}

/// `is_numeric_string_ex`: classifies a string as an integer (IS_LONG), float (IS_DOUBLE) or
/// neither (0), storing the value in `lval`/`dval`. `allow_errors` accepts trailing data, reporting
/// it through `trailing_data`; `oflow_info` receives the integer overflow direction.
#[no_mangle]
pub unsafe extern "C" fn is_numeric_string_ex(
    str: *const c_char,
    length: usize,
    lval: *mut zend_long,
    dval: *mut f64,
    allow_errors: bool,
    oflow_info: *mut c_int,
    trailing_data: *mut bool,
) -> u8 {
    let text = if length == 0 { &[][..] } else { std::slice::from_raw_parts(str.cast::<u8>(), length) };
    if !oflow_info.is_null() {
        *oflow_info = 0;
    }
    if !trailing_data.is_null() {
        *trailing_data = false;
    }
    let Some((number, overflow, trailing)) = parse_numeric(text, allow_errors) else { return 0 };
    if !oflow_info.is_null() {
        *oflow_info = overflow;
    }
    if !trailing_data.is_null() {
        *trailing_data = trailing;
    }
    match number {
        Numeric::Long(value) => {
            if !lval.is_null() {
                *lval = value;
            }
            IS_LONG
        }
        Numeric::Double(value) => {
            if !dval.is_null() {
                *dval = value;
            }
            IS_DOUBLE
        }
    }
}

/// `is_numeric_string`: `is_numeric_string_ex` without overflow and trailing-data reporting.
#[no_mangle]
pub unsafe extern "C" fn is_numeric_string(
    str: *const c_char,
    length: usize,
    lval: *mut zend_long,
    dval: *mut f64,
    allow_errors: bool,
) -> u8 {
    is_numeric_string_ex(str, length, lval, dval, allow_errors, std::ptr::null_mut(), std::ptr::null_mut())
}

/// `is_numeric_str_function`: classifies a `zend_string` as numeric; trailing data makes it not
/// numeric.
#[no_mangle]
pub unsafe extern "C" fn is_numeric_str_function(str: *const zend_string, lval: *mut zend_long, dval: *mut f64) -> u8 {
    // php-src passes `allow_errors = false`: "12abc" is not numeric here.
    is_numeric_string_ex((*str).val.as_ptr(), (*str).len, lval, dval, false, std::ptr::null_mut(), std::ptr::null_mut())
}

/// `(int)` of a float: 0 for NAN and infinities, the value modulo 2^64 when
/// it does not fit, as PHP 8 on 64-bit platforms.
#[no_mangle]
pub extern "C" fn zend_dval_to_lval(d: f64) -> zend_long {
    if !d.is_finite() {
        return 0;
    }
    if d < 9.223372036854775808e18 && d >= -9.223372036854775808e18 {
        return d as zend_long;
    }
    let two_pow_64 = 18446744073709551616.0f64;
    let two_pow_63 = 9223372036854775808.0f64;
    let mut dmod = d % two_pow_64;
    if dmod < 0.0 {
        if dmod == -two_pow_63 {
            return ZEND_LONG_MIN;
        }
        dmod += two_pow_64;
    }
    // 2^63 itself wraps to ZEND_LONG_MIN, as php-src's unsigned cast does.
    if dmod >= two_pow_63 {
        dmod -= two_pow_64;
    }
    dmod as zend_long
}

/// `zend_dval_to_lval_cap`: converts a double to an integer saturating at the integer limits; any
/// non-finite value (NaN, +INF, -INF) becomes 0, as in php-src. PHP 8.5 passes the source string
/// as a second argument, which only the incompatible-cast diagnostics would use.
#[no_mangle]
pub extern "C" fn zend_dval_to_lval_cap(d: f64, _s: *const zend_string) -> zend_long {
    if !d.is_finite() {
        0
    } else if d >= 9.223372036854775808e18 {
        ZEND_LONG_MAX
    } else if d < -9.223372036854775808e18 {
        ZEND_LONG_MIN
    } else {
        d as zend_long
    }
}

/// `zend_atol`: parses a numeric string to an integer, converting floats with saturation; 0 when
/// the text is not numeric.
#[no_mangle]
pub unsafe extern "C" fn zend_atol(str: *const c_char, str_len: usize) -> zend_long {
    let text = std::slice::from_raw_parts(str.cast::<u8>(), str_len);
    match parse_numeric(text, true) {
        Some((Numeric::Long(value), _, _)) => value,
        Some((Numeric::Double(value), _, _)) => zend_dval_to_lval_cap(value, std::ptr::null()),
        None => 0,
    }
}

/// `zend_atoi`: `zend_atol` narrowed to `int`.
#[no_mangle]
pub unsafe extern "C" fn zend_atoi(str: *const c_char, str_len: usize) -> c_int {
    zend_atol(str, str_len) as c_int
}

// ------------------------------------------------------------ zval conversions

/// The number a string holds in weak mode, `None` when it holds none.
pub unsafe fn string_number(s: *const zend_string) -> Option<Numeric> {
    parse_numeric(bytes(s), true).map(|(number, _, _)| number)
}

/// `zval_get_long_func`: the integer value of any zval following PHP's conversion rules, without
/// modifying the zval; a strict caller is told when a float-string loses precision.
#[no_mangle]
pub unsafe extern "C" fn zval_get_long_func(op: *const zval, is_strict: bool) -> zend_long {
    let op = resolve(op as *mut zval);
    match type_of(op) {
        IS_TRUE => 1,
        IS_LONG => (*op).value.lval,
        IS_DOUBLE => zend_dval_to_lval((*op).value.dval),
        IS_STRING => match string_number((*op).value.str) {
            Some(Numeric::Long(value)) => value,
            Some(Numeric::Double(value)) => {
                let lval = zend_dval_to_lval_cap(value, std::ptr::null());
                // As php-src: only a strict caller hears that the float-string lost precision.
                if is_strict && !(value.is_finite() && lval as f64 == value) {
                    zend_incompatible_string_to_long_error((*op).value.str);
                }
                lval
            }
            None => 0,
        },
        IS_ARRAY => ((*(*op).value.arr).nNumOfElements > 0) as zend_long,
        IS_OBJECT => object_cast(op, IS_LONG).map_or(1, |value| value.value.lval),
        IS_RESOURCE => (*(*op).value.res).handle,
        _ => 0,
    }
}

/// php-src's `convert_object_to_type`: the object's `cast_object` answer as `kind` (`IS_LONG` or
/// `IS_DOUBLE`), as GMP's and SimpleXML's handlers give one; None, after PHP's "could not be
/// converted" warning when the handler refuses, for the caller's fallback of one.
unsafe fn object_cast(op: *mut zval, kind: u8) -> Option<zval> {
    let object = (*op).value.obj;
    let mut converted = zval::UNDEF;
    let cast = (*(*object).handlers).cast_object;
    if !cast.is_some_and(|cast| cast(object, &mut converted, kind as c_int) == SUCCESS) {
        let target = if kind == IS_LONG { "int" } else { "float" };
        crate::error::warning(&format!(
            "Object of class {} could not be converted to {target}",
            string::text((*(*object).ce).name)
        ));
        return None;
    }
    if type_of(&converted) == kind {
        Some(converted)
    } else {
        zval_ptr_dtor(&mut converted);
        None
    }
}

/// `zval_get_long`: integer value of a zval, not strict.
#[no_mangle]
pub unsafe extern "C" fn zval_get_long(op: *const zval) -> zend_long {
    zval_get_long_func(op, false)
}

/// `zval_get_long_ex`: integer value of a zval, with php-src's explicit `is_strict` flag.
#[no_mangle]
pub unsafe extern "C" fn zval_get_long_ex(op: *const zval, is_strict: bool) -> zend_long {
    zval_get_long_func(op, is_strict)
}

/// `zval_get_double_func`: the float value of any zval following PHP's conversion rules, without
/// modifying the zval.
#[no_mangle]
pub unsafe extern "C" fn zval_get_double_func(op: *const zval) -> f64 {
    let op = resolve(op as *mut zval);
    match type_of(op) {
        IS_TRUE => 1.0,
        IS_LONG => (*op).value.lval as f64,
        IS_DOUBLE => (*op).value.dval,
        IS_STRING => match string_number((*op).value.str) {
            Some(Numeric::Long(value)) => value as f64,
            Some(Numeric::Double(value)) => value,
            None => 0.0,
        },
        IS_ARRAY => ((*(*op).value.arr).nNumOfElements > 0) as i32 as f64,
        IS_OBJECT => object_cast(op, IS_DOUBLE).map_or(1.0, |value| value.value.dval),
        IS_RESOURCE => (*(*op).value.res).handle as f64,
        _ => 0.0,
    }
}

/// `zval_get_double`: float value of a zval.
#[no_mangle]
pub unsafe extern "C" fn zval_get_double(op: *const zval) -> f64 {
    zval_get_double_func(op)
}

/// `(string)$value`, with PHP's warning for arrays and `__toString` for
/// objects. NULL (an exception pending) when an object has no string form.
pub unsafe fn to_string(op: *mut zval, try_mode: bool) -> *mut zend_string {
    let op = resolve(op);
    match type_of(op) {
        IS_UNDEF | IS_NULL | IS_FALSE => crate::string::zend_empty_string,
        IS_TRUE => crate::string::zend_one_char_string[b'1' as usize],
        IS_LONG => string::zend_long_to_str((*op).value.lval),
        IS_DOUBLE => string::zend_double_to_str((*op).value.dval),
        IS_STRING => string::zend_string_copy((*op).value.str),
        IS_ARRAY => {
            crate::error::warning("Array to string conversion");
            if try_mode && !crate::startup::eg().exception.is_null() {
                return std::ptr::null_mut();
            }
            string::intern(b"Array")
        }
        IS_RESOURCE => string::new(format!("Resource id #{}", (*(*op).value.res).handle).as_bytes()),
        IS_OBJECT => {
            let object = (*op).value.obj;
            let mut result = zval::UNDEF;
            if let Some(cast) = (*(*object).handlers).cast_object {
                if cast(object, &mut result, IS_STRING as c_int) == SUCCESS && type_of(&result) == IS_STRING {
                    return result.value.str;
                }
            }
            if crate::startup::eg().exception.is_null() {
                crate::exception::throw_error(
                    crate::exception::zend_ce_error,
                    &format!("Object of class {} could not be converted to string", string::text((*(*object).ce).name)),
                );
            }
            if try_mode {
                std::ptr::null_mut()
            } else {
                crate::string::zend_empty_string
            }
        }
        _ => crate::string::zend_empty_string,
    }
}

/// `zval_get_string_func`: the string form of a zval as a string the caller must release; arrays
/// give "Array" with a warning, and an object that cannot be converted throws Error and yields the
/// empty string.
#[no_mangle]
pub unsafe extern "C" fn zval_get_string_func(op: *mut zval) -> *mut zend_string {
    to_string(op, false)
}

/// `zval_try_get_string_func`: like `zval_get_string_func`, but returns NULL when the conversion
/// throws.
#[no_mangle]
pub unsafe extern "C" fn zval_try_get_string_func(op: *mut zval) -> *mut zend_string {
    to_string(op, true)
}

/// `zval_get_string`: string form of a zval, which the caller must release.
#[no_mangle]
pub unsafe extern "C" fn zval_get_string(op: *mut zval) -> *mut zend_string {
    to_string(op, false)
}

/// `zval_try_get_string`: string form of a zval, or NULL if the conversion throws.
#[no_mangle]
pub unsafe extern "C" fn zval_try_get_string(op: *mut zval) -> *mut zend_string {
    to_string(op, true)
}

/// A string view: `*tmp` holds the reference to release, or NULL when the
/// value already was a string.
#[no_mangle]
pub unsafe extern "C" fn zval_get_tmp_string(op: *mut zval, tmp: *mut *mut zend_string) -> *mut zend_string {
    if type_of(op) == IS_STRING {
        *tmp = std::ptr::null_mut();
        return (*op).value.str;
    }
    let s = to_string(op, false);
    *tmp = s;
    s
}

/// `zval_try_get_tmp_string`: string form of a zval for temporary use. A zval that already is a
/// string is returned as is and `*tmp` is set to NULL; otherwise `*tmp` holds the new string to
/// release with `zend_tmp_string_release`. NULL on a throwing conversion.
#[no_mangle]
pub unsafe extern "C" fn zval_try_get_tmp_string(op: *mut zval, tmp: *mut *mut zend_string) -> *mut zend_string {
    if type_of(op) == IS_STRING {
        *tmp = std::ptr::null_mut();
        return (*op).value.str;
    }
    let s = to_string(op, true);
    *tmp = s;
    s
}

/// `zend_tmp_string_release`: releases the temporary string from `zval_try_get_tmp_string` (NULL is
/// a no-op).
#[no_mangle]
pub unsafe extern "C" fn zend_tmp_string_release(tmp: *mut zend_string) {
    if !tmp.is_null() {
        string::zend_string_release(tmp);
    }
}

/// `zend_is_true`: PHP truthiness of a zval (false, 0, 0.0, "", "0", empty array and null are
/// false).
#[no_mangle]
pub unsafe extern "C" fn zend_is_true(op: *const zval) -> bool {
    let op = resolve(op as *mut zval);
    match type_of(op) {
        IS_TRUE => true,
        IS_LONG => (*op).value.lval != 0,
        IS_DOUBLE => (*op).value.dval != 0.0,
        IS_STRING => {
            let text = bytes((*op).value.str);
            !(text.is_empty() || text == b"0")
        }
        IS_ARRAY => (*(*op).value.arr).nNumOfElements > 0,
        IS_OBJECT => zend_object_is_true(op),
        IS_RESOURCE => true,
        _ => false,
    }
}

/// `zend_object_is_true`: truthiness of an object zval, asked of its `cast_object` handler as
/// php-src does (GMP and SimpleXML objects can be false); an object that cannot be converted
/// raises PHP's recoverable error and is false.
#[no_mangle]
pub unsafe extern "C" fn zend_object_is_true(op: *const zval) -> bool {
    let object = (*op).value.obj;
    let mut converted = zval::UNDEF;
    if let Some(cast) = (*(*object).handlers).cast_object {
        if cast(object, &mut converted, _IS_BOOL as c_int) == SUCCESS {
            return type_of(&converted) == IS_TRUE;
        }
    }
    let name = string::text((*(*object).ce).name);
    crate::error::report(
        E_RECOVERABLE_ERROR,
        format!("Object of class {name} could not be converted to bool").as_bytes(),
    );
    false
}

/// `i_zend_is_true`: inline spelling of `zend_is_true`.
#[no_mangle]
pub unsafe extern "C" fn i_zend_is_true(op: *const zval) -> bool {
    zend_is_true(op)
}

/// `convert_to_long`: converts a zval to an integer in place, releasing its previous value.
#[no_mangle]
pub unsafe extern "C" fn convert_to_long(op: *mut zval) {
    let value = zval_get_long_func(op, false);
    zval_ptr_dtor(op);
    set_long(op, value);
}

/// `convert_to_double`: converts a zval to a float in place, releasing its previous value.
#[no_mangle]
pub unsafe extern "C" fn convert_to_double(op: *mut zval) {
    let value = zval_get_double_func(op);
    zval_ptr_dtor(op);
    set_double(op, value);
}

/// `convert_to_boolean`: converts a zval to a boolean in place, releasing its previous value.
#[no_mangle]
pub unsafe extern "C" fn convert_to_boolean(op: *mut zval) {
    let value = zend_is_true(op);
    zval_ptr_dtor(op);
    set_bool(op, value);
}

/// `convert_to_null`: releases a zval's value and sets it to null.
#[no_mangle]
pub unsafe extern "C" fn convert_to_null(op: *mut zval) {
    zval_ptr_dtor(op);
    set_null(op);
}

/// `_convert_to_string`: converts a zval to a string in place, releasing its previous value.
#[no_mangle]
pub unsafe extern "C" fn _convert_to_string(op: *mut zval) {
    let value = to_string(op, false);
    zval_ptr_dtor(op);
    set_str(op, value);
}

/// `convert_to_string`: converts a zval to a string in place unless it already is one.
#[no_mangle]
pub unsafe extern "C" fn convert_to_string(op: *mut zval) {
    if type_of(op) != IS_STRING {
        _convert_to_string(op);
    }
}

/// `_try_convert_to_string`: converts a zval to a string in place; returns false when the
/// conversion throws.
#[no_mangle]
pub unsafe extern "C" fn _try_convert_to_string(op: *mut zval) -> bool {
    let value = to_string(op, true);
    if value.is_null() {
        return false;
    }
    zval_ptr_dtor(op);
    set_str(op, value);
    true
}

/// `convert_to_array`: converts a zval to an array in place (null becomes an empty array, objects
/// become a copy of their properties, scalars become one-element arrays).
#[no_mangle]
pub unsafe extern "C" fn convert_to_array(op: *mut zval) {
    match type_of(op) {
        IS_ARRAY => {}
        IS_NULL | IS_UNDEF => set_arr(op, crate::hash::_zend_new_array(0)),
        IS_OBJECT => {
            let object = (*op).value.obj;
            let properties = crate::object::zend_elephc_object_properties(object);
            let copy = crate::hash::zend_array_dup(properties);
            zval_ptr_dtor(op);
            set_arr(op, copy);
        }
        _ => {
            let arr = crate::hash::_zend_new_array(1);
            let mut value = zval::UNDEF;
            copy_value(&mut value, op);
            crate::hash::zend_hash_next_index_insert(arr, &mut value);
            set_arr(op, arr);
        }
    }
}

/// `convert_to_object`: converts a zval to an object in place (arrays become the properties of a
/// new plain object, null gives an empty object, scalars are stored under the `scalar` property).
#[no_mangle]
pub unsafe extern "C" fn convert_to_object(op: *mut zval) {
    match type_of(op) {
        IS_OBJECT => {}
        IS_ARRAY => {
            let arr = (*op).value.arr;
            crate::object::object_init(op);
            let properties = crate::object::zend_elephc_object_properties((*op).value.obj);
            crate::hash::zend_hash_copy(properties, arr, Some(crate::zval::zval_add_ref));
            crate::hash::zend_array_release(arr);
        }
        IS_NULL | IS_UNDEF => crate::object::object_init(op),
        _ => {
            let mut value = zval::UNDEF;
            copy_value(&mut value, op);
            crate::object::object_init(op);
            let properties = crate::object::zend_elephc_object_properties((*op).value.obj);
            crate::hash::zend_hash_str_update(properties, c"scalar".as_ptr(), 6, &mut value);
        }
    }
}

/// `convert_scalar_to_number`: converts a scalar zval in place to an int or float, parsing strings
/// by their numeric prefix (0 when there is none).
#[no_mangle]
pub unsafe extern "C" fn convert_scalar_to_number(op: *mut zval) {
    match type_of(op) {
        IS_LONG | IS_DOUBLE => {}
        IS_STRING => match string_number((*op).value.str) {
            Some(Numeric::Double(value)) => {
                zval_ptr_dtor(op);
                set_double(op, value);
            }
            Some(Numeric::Long(value)) => {
                zval_ptr_dtor(op);
                set_long(op, value);
            }
            None => {
                zval_ptr_dtor(op);
                set_long(op, 0);
            }
        },
        _ => convert_to_long(op),
    }
}

// ------------------------------------------------------------ comparison

/// Maps a Rust `Ordering` to the -1/0/1 integer PHP comparison functions return.
fn ordering(o: Ordering) -> c_int {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// Three-way compares two doubles as PHP does; an unordered pair (NaN) compares as 1.
fn compare_doubles(a: f64, b: f64) -> c_int {
    a.partial_cmp(&b).map(ordering).unwrap_or(1)
}

/// The value as a number, for numeric comparison.
unsafe fn number_of(op: *mut zval) -> Numeric {
    match type_of(op) {
        IS_LONG => Numeric::Long((*op).value.lval),
        IS_DOUBLE => Numeric::Double((*op).value.dval),
        IS_STRING => string_number((*op).value.str).unwrap_or(Numeric::Long(0)),
        _ => Numeric::Long(zval_get_long_func(op, false)),
    }
}

/// Three-way compares two numbers, mixing int and float by converting the int to a double.
fn compare_numbers(a: Numeric, b: Numeric) -> c_int {
    match (a, b) {
        (Numeric::Long(a), Numeric::Long(b)) => ordering(a.cmp(&b)),
        (Numeric::Long(a), Numeric::Double(b)) => compare_doubles(a as f64, b),
        (Numeric::Double(a), Numeric::Long(b)) => compare_doubles(a, b as f64),
        (Numeric::Double(a), Numeric::Double(b)) => compare_doubles(a, b),
    }
}

/// `$a <=> $b`.
#[no_mangle]
pub unsafe extern "C" fn zend_compare(op1: *mut zval, op2: *mut zval) -> c_int {
    let (a, b) = (resolve(op1), resolve(op2));
    let (ta, tb) = (type_of(a), type_of(b));
    match (ta, tb) {
        (IS_LONG | IS_DOUBLE, IS_LONG | IS_DOUBLE) => compare_numbers(number_of(a), number_of(b)),
        (IS_STRING, IS_STRING) => {
            let (sa, sb) = ((*a).value.str, (*b).value.str);
            if sa == sb {
                return 0;
            }
            match (parse_numeric(bytes(sa), false), parse_numeric(bytes(sb), false)) {
                (Some((x, _, _)), Some((y, _, _))) => compare_numbers(x, y),
                _ => ordering(bytes(sa).cmp(bytes(sb))),
            }
        }
        (IS_STRING, IS_LONG | IS_DOUBLE) | (IS_LONG | IS_DOUBLE, IS_STRING) => {
            let (s, n, flip) = if ta == IS_STRING { (a, b, false) } else { (b, a, true) };
            let result = match parse_numeric(bytes((*s).value.str), false) {
                Some((x, _, _)) => compare_numbers(x, number_of(n)),
                None => {
                    let text = to_string(n, false);
                    let r = ordering(bytes((*s).value.str).cmp(bytes(text)));
                    string::zend_string_release(text);
                    r
                }
            };
            if flip {
                -result
            } else {
                result
            }
        }
        // php-src orders an object above null before anything else, then hands any
        // other pair holding an object to that object's `compare` handler (the left
        // one's when both are objects), ahead of the bool and array rules.
        (IS_OBJECT, IS_NULL | IS_UNDEF) => 1,
        (IS_NULL | IS_UNDEF, IS_OBJECT) => -1,
        (IS_OBJECT, _) | (_, IS_OBJECT) => {
            if ta == IS_OBJECT && tb == IS_OBJECT && (*a).value.obj == (*b).value.obj {
                return 0;
            }
            let object = if ta == IS_OBJECT { a } else { b };
            match (*(*(*object).value.obj).handlers).compare {
                Some(compare) => compare(a, b),
                None => 1,
            }
        }
        (IS_NULL | IS_UNDEF, IS_NULL | IS_UNDEF) => 0,
        (IS_NULL | IS_UNDEF, IS_STRING) => ordering(0.cmp(&bytes((*b).value.str).len())),
        (IS_STRING, IS_NULL | IS_UNDEF) => ordering(bytes((*a).value.str).len().cmp(&0)),
        (IS_NULL | IS_UNDEF | IS_FALSE | IS_TRUE, _) | (_, IS_NULL | IS_UNDEF | IS_FALSE | IS_TRUE) => {
            ordering(zend_is_true(a).cmp(&zend_is_true(b)))
        }
        (IS_ARRAY, IS_ARRAY) => crate::hash::zend_hash_compare(
            (*a).value.arr,
            (*b).value.arr,
            Some(compare_zvals_callback),
            false,
        ),
        (IS_ARRAY, _) => 1,
        (_, IS_ARRAY) => -1,
        _ => compare_numbers(number_of(a), number_of(b)),
    }
}

/// Comparator adaptor handing two zval pointers to `zend_compare`, for hash comparison callbacks.
unsafe extern "C" fn compare_zvals_callback(a: *const std::ffi::c_void, b: *const std::ffi::c_void) -> c_int {
    zend_compare(a as *mut zval, b as *mut zval)
}

/// `compare_function`: stores the result of `zend_compare(op1, op2)` as an integer in `result` and
/// returns SUCCESS.
#[no_mangle]
pub unsafe extern "C" fn compare_function(result: *mut zval, op1: *mut zval, op2: *mut zval) -> zend_result {
    set_long(result, zend_compare(op1, op2) as zend_long);
    SUCCESS
}

/// `numeric_compare_function`: compares two zvals as floats.
#[no_mangle]
pub unsafe extern "C" fn numeric_compare_function(op1: *mut zval, op2: *mut zval) -> c_int {
    compare_doubles(zval_get_double_func(op1), zval_get_double_func(op2))
}

/// `string_compare_function`: compares two zvals as byte strings after string conversion, returning
/// -1/0/1.
#[no_mangle]
pub unsafe extern "C" fn string_compare_function(op1: *mut zval, op2: *mut zval) -> c_int {
    let (a, b) = (to_string(op1, false), to_string(op2, false));
    let result = ordering(bytes(a).cmp(bytes(b)));
    string::zend_string_release(a);
    string::zend_string_release(b);
    result
}

/// `string_case_compare_function`: compares two zvals as strings ignoring ASCII case.
#[no_mangle]
pub unsafe extern "C" fn string_case_compare_function(op1: *mut zval, op2: *mut zval) -> c_int {
    let (a, b) = (to_string(op1, false), to_string(op2, false));
    let result = string::zend_binary_strcasecmp((*a).val.as_ptr(), (*a).len, (*b).val.as_ptr(), (*b).len);
    string::zend_string_release(a);
    string::zend_string_release(b);
    result.signum()
}

/// `string_locale_compare_function`: locale comparison, which here is the plain byte comparison.
#[no_mangle]
pub unsafe extern "C" fn string_locale_compare_function(op1: *mut zval, op2: *mut zval) -> c_int {
    string_compare_function(op1, op2)
}

/// `$a === $b`.
#[no_mangle]
pub unsafe extern "C" fn zend_is_identical(op1: *const zval, op2: *const zval) -> bool {
    let (a, b) = (resolve(op1 as *mut zval), resolve(op2 as *mut zval));
    if type_of(a) != type_of(b) {
        return false;
    }
    match type_of(a) {
        IS_UNDEF | IS_NULL | IS_FALSE | IS_TRUE => true,
        IS_LONG => (*a).value.lval == (*b).value.lval,
        IS_DOUBLE => (*a).value.dval == (*b).value.dval,
        IS_STRING => string::zend_string_equals((*a).value.str, (*b).value.str),
        IS_ARRAY => {
            (*a).value.arr == (*b).value.arr
                || crate::hash::zend_hash_compare((*a).value.arr, (*b).value.arr, Some(identical_callback), true) == 0
        }
        IS_OBJECT => (*a).value.obj == (*b).value.obj,
        IS_RESOURCE => (*a).value.res == (*b).value.res,
        _ => false,
    }
}

/// Hash comparison callback returning 0 when two zvals are identical and non-zero otherwise.
unsafe extern "C" fn identical_callback(a: *const std::ffi::c_void, b: *const std::ffi::c_void) -> c_int {
    (!zend_is_identical(a.cast(), b.cast())) as c_int
}

/// `fast_is_identical_function`: strict `===` comparison of two zvals.
#[no_mangle]
pub unsafe extern "C" fn fast_is_identical_function(op1: *mut zval, op2: *mut zval) -> bool {
    zend_is_identical(op1, op2)
}

// ------------------------------------------------------------ arithmetic

/// One operand of `+ - * /` as a number, following php-src's operand rules: null, bool, int and
/// float convert; a numeric string converts, and a leading-numeric one (`"12abc"`) warns
/// `A non-numeric value encountered` and uses its prefix; a non-numeric string, an array, an
/// object or a resource throws `TypeError: Unsupported operand types: <left> <op> <right>` and
/// yields `None`.
unsafe fn arithmetic_operand(operand: *mut zval, left: *mut zval, right: *mut zval, op: char) -> Option<Numeric> {
    let unsupported = || {
        let name = |value: *mut zval| CStr::from_ptr(zend_zval_value_name(value)).to_string_lossy().into_owned();
        crate::exception::throw_error(
            crate::exception::zend_ce_type_error,
            &format!("Unsupported operand types: {} {op} {}", name(left), name(right)),
        );
        None
    };
    match type_of(operand) {
        IS_STRING => match parse_numeric(bytes((*operand).value.str), true) {
            Some((number, _, trailing)) => {
                if trailing {
                    crate::error::report(E_WARNING, b"A non-numeric value encountered");
                }
                Some(number)
            }
            None => unsupported(),
        },
        IS_ARRAY | IS_OBJECT | IS_RESOURCE => unsupported(),
        _ => Some(number_of(operand)),
    }
}

/// Shared arithmetic for the operator functions: converts both operands to numbers and stores the
/// result of `+ - * /` in `result`, overflowing integers to floats; two ints divide in integer
/// arithmetic, an exact quotient staying an int. Division by zero throws DivisionByZeroError and returns FAILURE; `result` may
/// alias an operand.
unsafe fn arithmetic(result: *mut zval, op1: *mut zval, op2: *mut zval, op: char) -> zend_result {
    let (left, right) = (resolve(op1), resolve(op2));
    // php-src's `ZEND_TRY_BINARY_OP{1,2}_OBJECT_OPERATION`: an object operand whose handlers
    // overload the operator (GMP-style) computes it, the left one first.
    let opcode = match op {
        '+' => 1, // ZEND_ADD
        '-' => 2, // ZEND_SUB
        '*' => 3, // ZEND_MUL
        _ => 4,   // ZEND_DIV
    };
    for operand in [left, right] {
        if type_of(operand) == IS_OBJECT {
            if let Some(operation) = (*(*(*operand).value.obj).handlers).do_operation {
                if operation(opcode, result, left, right) == SUCCESS {
                    return SUCCESS;
                }
            }
        }
    }
    let (Some(a), Some(b)) = (arithmetic_operand(left, left, right, op), arithmetic_operand(right, left, right, op)) else {
        return FAILURE;
    };
    let mut value = zval::UNDEF;
    match (a, b, op) {
        (Numeric::Long(x), Numeric::Long(y), '+') => match x.checked_add(y) {
            Some(v) => set_long(&mut value, v),
            None => set_double(&mut value, x as f64 + y as f64),
        },
        (Numeric::Long(x), Numeric::Long(y), '-') => match x.checked_sub(y) {
            Some(v) => set_long(&mut value, v),
            None => set_double(&mut value, x as f64 - y as f64),
        },
        (Numeric::Long(x), Numeric::Long(y), '*') => match x.checked_mul(y) {
            Some(v) => set_long(&mut value, v),
            None => set_double(&mut value, x as f64 * y as f64),
        },
        // php-src's integer division: an exact quotient stays an int, LONG_MIN / -1
        // (which would trap) and inexact quotients become floats.
        (Numeric::Long(x), Numeric::Long(y), '/') => {
            if y == 0 {
                crate::exception::throw_error(crate::exception::zend_ce_division_by_zero_error, "Division by zero");
                return FAILURE;
            }
            if y == -1 && x == zend_long::MIN {
                set_double(&mut value, zend_long::MIN as f64 / -1.0);
            } else if x % y == 0 {
                set_long(&mut value, x / y);
            } else {
                set_double(&mut value, x as f64 / y as f64);
            }
        }
        (x, y, op) => {
            let x = match x {
                Numeric::Long(v) => v as f64,
                Numeric::Double(v) => v,
            };
            let y = match y {
                Numeric::Long(v) => v as f64,
                Numeric::Double(v) => v,
            };
            if op == '/' && y == 0.0 {
                crate::exception::throw_error(crate::exception::zend_ce_division_by_zero_error, "Division by zero");
                return FAILURE;
            }
            let v = match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                _ => x / y,
            };
            set_double(&mut value, v);
        }
    }
    if result == op1 || result == op2 {
        let mut old = zval::UNDEF;
        copy_value(&mut old, result);
        copy_value(result, &value);
        zval_ptr_dtor(&mut old);
    } else {
        copy_value(result, &value);
    }
    SUCCESS
}

/// `$a + $b`: numbers add (overflowing into float), arrays union.
#[no_mangle]
pub unsafe extern "C" fn add_function(result: *mut zval, op1: *mut zval, op2: *mut zval) -> zend_result {
    let (a, b) = (resolve(op1), resolve(op2));
    if type_of(a) == IS_ARRAY && type_of(b) == IS_ARRAY {
        let union = crate::hash::zend_array_dup((*a).value.arr);
        crate::hash::zend_hash_merge(union, (*b).value.arr, Some(crate::zval::zval_add_ref), false);
        if result == op1 || result == op2 {
            zval_ptr_dtor(result);
        }
        set_arr(result, union);
        return SUCCESS;
    }
    arithmetic(result, op1, op2, '+')
}

/// `sub_function`: stores `op1 - op2` in `result`.
#[no_mangle]
pub unsafe extern "C" fn sub_function(result: *mut zval, op1: *mut zval, op2: *mut zval) -> zend_result {
    arithmetic(result, op1, op2, '-')
}

/// `mul_function`: stores `op1 * op2` in `result`.
#[no_mangle]
pub unsafe extern "C" fn mul_function(result: *mut zval, op1: *mut zval, op2: *mut zval) -> zend_result {
    arithmetic(result, op1, op2, '*')
}

/// `div_function`: stores `op1 / op2` in `result`; division by zero throws DivisionByZeroError.
#[no_mangle]
pub unsafe extern "C" fn div_function(result: *mut zval, op1: *mut zval, op2: *mut zval) -> zend_result {
    arithmetic(result, op1, op2, '/')
}

/// `concat_function`: stores the string concatenation of both operands, each converted to a string,
/// in `result`.
#[no_mangle]
pub unsafe extern "C" fn concat_function(result: *mut zval, op1: *mut zval, op2: *mut zval) -> zend_result {
    let (a, b) = (to_string(op1, false), to_string(op2, false));
    let mut joined = bytes(a).to_vec();
    joined.extend_from_slice(bytes(b));
    string::zend_string_release(a);
    string::zend_string_release(b);
    if result == op1 || result == op2 {
        zval_ptr_dtor(result);
    }
    set_bytes(result, &joined);
    SUCCESS
}

// ------------------------------------------------------------ type names

/// `zend_get_type_by_const`: the declared-type spelling of a type code.
pub fn type_name(code: u8) -> &'static str {
    match code {
        IS_FALSE | IS_TRUE | _IS_BOOL => "bool",
        IS_LONG => "int",
        IS_DOUBLE => "float",
        IS_STRING => "string",
        IS_OBJECT => "object",
        IS_RESOURCE => "resource",
        IS_NULL => "null",
        IS_CALLABLE => "callable",
        IS_ITERABLE => "iterable",
        IS_ARRAY => "array",
        IS_VOID => "void",
        IS_MIXED => "mixed",
        _IS_NUMBER => "int|float",
        _ => "unknown",
    }
}

/// How an error message names a value: its class for an object, `true` or
/// `false` for a bool, otherwise its type.
pub unsafe fn value_name(arg: *const zval) -> String {
    let arg = resolve(arg as *mut zval);
    match type_of(arg) {
        IS_OBJECT => string::text((*(*(*arg).value.obj).ce).name),
        IS_FALSE => "false".to_string(),
        IS_TRUE => "true".to_string(),
        IS_UNDEF => "null".to_string(),
        code => type_name(code).to_string(),
    }
}

static TYPE_NAMES: [&[u8]; 16] = [
    b"null\0", b"null\0", b"bool\0", b"bool\0", b"int\0", b"float\0", b"string\0", b"array\0", b"object\0",
    b"resource\0", b"reference\0", b"constant expression\0", b"indirect\0", b"pointer\0", b"alias\0", b"error\0",
];

/// `zend_get_type_by_const`: the PHP type name for an `IS_*` type constant.
#[no_mangle]
pub unsafe extern "C" fn zend_get_type_by_const(r#type: c_int) -> *const c_char {
    match r#type as u8 {
        IS_CALLABLE => c"callable".as_ptr(),
        IS_ITERABLE => c"iterable".as_ptr(),
        IS_VOID => c"void".as_ptr(),
        IS_MIXED => c"mixed".as_ptr(),
        _IS_BOOL => c"bool".as_ptr(),
        _IS_NUMBER => c"int|float".as_ptr(),
        code if (code as usize) < TYPE_NAMES.len() => TYPE_NAMES[code as usize].as_ptr().cast(),
        _ => c"unknown".as_ptr(),
    }
}

/// `zend_zval_type_name`: the PHP type name of a zval, resolving references.
#[no_mangle]
pub unsafe extern "C" fn zend_zval_type_name(arg: *const zval) -> *const c_char {
    let arg = resolve(arg as *mut zval);
    TYPE_NAMES[(type_of(arg) as usize).min(15)].as_ptr().cast()
}

/// The class name of an object (interned, so the pointer stays valid), or the
/// type name.
#[no_mangle]
pub unsafe extern "C" fn zend_zval_value_name(arg: *const zval) -> *const c_char {
    let resolved = resolve(arg as *mut zval);
    match type_of(resolved) {
        IS_OBJECT => (*(*(*(*resolved).value.obj).ce).name).val.as_ptr(),
        IS_FALSE => c"false".as_ptr(),
        IS_TRUE => c"true".as_ptr(),
        _ => zend_zval_type_name(arg),
    }
}

/// `gettype()`'s names.
#[no_mangle]
pub unsafe extern "C" fn zend_zval_get_legacy_type(arg: *const zval) -> *mut zend_string {
    let arg = resolve(arg as *mut zval);
    let name: &[u8] = match type_of(arg) {
        IS_NULL | IS_UNDEF => b"NULL",
        IS_FALSE | IS_TRUE => b"boolean",
        IS_LONG => b"integer",
        IS_DOUBLE => b"double",
        IS_STRING => b"string",
        IS_ARRAY => b"array",
        IS_OBJECT => b"object",
        IS_RESOURCE => b"resource",
        _ => b"unknown type",
    };
    string::intern(name)
}

/// `zend_is_iterable`: whether a zval is an array or a Traversable object.
#[no_mangle]
pub unsafe extern "C" fn zend_is_iterable(iterable: *const zval) -> bool {
    let value = resolve(iterable as *mut zval);
    match type_of(value) {
        IS_ARRAY => true,
        IS_OBJECT => crate::class::instanceof_function_slow((*(*value).value.obj).ce, crate::class::zend_ce_traversable),
        _ => false,
    }
}

/// `zend_is_countable`: whether a zval is an array or a Countable object.
#[no_mangle]
pub unsafe extern "C" fn zend_is_countable(countable: *const zval) -> bool {
    let value = resolve(countable as *mut zval);
    match type_of(value) {
        IS_ARRAY => true,
        IS_OBJECT => {
            (*(*(*value).value.obj).handlers).count_elements.is_some()
                || crate::class::instanceof_function_slow((*(*value).value.obj).ce, crate::class::zend_ce_countable)
        }
        _ => false,
    }
}

/// `zend_nan_coerced_to_type_warning`: emits PHP's warning that a NAN value was coerced to the
/// given type.
#[no_mangle]
pub extern "C" fn zend_nan_coerced_to_type_warning(r#type: u8) {
    crate::error::warning(&format!("unexpected NAN value was coerced to {}", type_name(r#type)));
}

/// `zend_incompatible_double_to_long_error`: emits PHP's deprecation for an implicit float-to-int
/// conversion that loses precision.
#[no_mangle]
pub extern "C" fn zend_incompatible_double_to_long_error(d: f64) {
    crate::error::deprecated(&format!(
        "Implicit conversion from float {} to int loses precision",
        double_repr(d, -1, false)
    ));
}

/// `zend_incompatible_string_to_long_error`: emits PHP's deprecation for an implicit
/// float-string-to-int conversion that loses precision.
#[no_mangle]
pub unsafe extern "C" fn zend_incompatible_string_to_long_error(s: *const zend_string) {
    crate::error::deprecated(&format!(
        "Implicit conversion from float-string \"{}\" to int loses precision",
        string::text(s)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies float formatting matches PHP for exponent switching, negative zero, precision 14
    /// rounding and the `var_export` style repr with and without a forced decimal point.
    #[test]
    fn formats_floats_like_php() {
        assert_eq!(double_to_php(1e15, 14), "1.0E+15");
        assert_eq!(double_to_php(1e14, 14), "1.0E+14");
        assert_eq!(double_to_php(0.0001, 14), "0.0001");
        assert_eq!(double_to_php(0.00001, 14), "1.0E-5");
        assert_eq!(double_to_php(-0.0, 14), "-0");
        assert_eq!(double_to_php(1.0 / 3.0, 14), "0.33333333333333");
        assert_eq!(double_to_php(1e100, 14), "1.0E+100");
        assert_eq!(double_to_php(0.1 + 0.2, 14), "0.3");
        assert_eq!(double_repr(1e15, -1, true), "1000000000000000.0");
        assert_eq!(double_repr(1e17, -1, true), "1.0E+17");
        assert_eq!(double_repr(1.5e-5, -1, true), "1.5E-5");
        assert_eq!(double_repr(123456789012345678.0, -1, true), "1.2345678901234568E+17");
        assert_eq!(double_repr(0.1, -1, false), "0.1");
        assert_eq!(double_repr(-0.0, -1, false), "-0");
        assert_eq!(double_repr(-0.0, -1, true), "-0.0");
    }

    /// Verifies numeric string parsing matches PHP: whitespace trimming, exponents, trailing data
    /// accepted only when allowed, lone dots rejected, and integer overflow becoming a float.
    #[test]
    fn reads_numeric_strings_like_php() {
        assert!(matches!(parse_numeric(b" 12 ", false), Some((Numeric::Long(12), 0, false))));
        assert!(matches!(parse_numeric(b"1e3", false), Some((Numeric::Double(v), 0, false)) if v == 1000.0));
        assert!(parse_numeric(b"12abc", false).is_none());
        assert!(matches!(parse_numeric(b"12abc", true), Some((Numeric::Long(12), 0, true))));
        assert!(parse_numeric(b"abc", true).is_none());
        assert!(parse_numeric(b".", true).is_none());
        assert!(matches!(parse_numeric(b".5", false), Some((Numeric::Double(v), _, _)) if v == 0.5));
        assert!(matches!(parse_numeric(b"9223372036854775808", false), Some((Numeric::Double(_), 1, false))));
    }

    /// `zend_dval_to_lval_cap` zeroes every non-finite value, as php-src does, and saturates the
    /// finite out-of-range ones.
    #[test]
    fn capped_conversion_zeroes_non_finite() {
        let none = std::ptr::null();
        assert_eq!(zend_dval_to_lval_cap(f64::INFINITY, none), 0);
        assert_eq!(zend_dval_to_lval_cap(f64::NEG_INFINITY, none), 0);
        assert_eq!(zend_dval_to_lval_cap(f64::NAN, none), 0);
        assert_eq!(zend_dval_to_lval_cap(1e300, none), ZEND_LONG_MAX);
        assert_eq!(zend_dval_to_lval_cap(-1e300, none), ZEND_LONG_MIN);
        assert_eq!(zend_dval_to_lval_cap(-2.5, none), -2);
    }

    /// The radix readers skip what php-src skips: `0x`/`0b` when present, and the first byte of
    /// an octal literal; `endptr` stays at the start when no digit was read.
    #[test]
    fn radix_strtod_skips_prefixes_like_php() {
        unsafe {
            let read = |f: unsafe extern "C" fn(*const c_char, *mut *const c_char) -> f64, text: &[u8]| {
                let mut end: *const c_char = std::ptr::null();
                let value = f(text.as_ptr().cast(), &mut end);
                (value, end.offset_from(text.as_ptr().cast()))
            };
            assert_eq!(read(zend_hex_strtod, b"0x1F\0"), (31.0, 4));
            assert_eq!(read(zend_hex_strtod, b"ff\0"), (255.0, 2));
            assert_eq!(read(zend_hex_strtod, b"0x\0"), (0.0, 0));
            assert_eq!(read(zend_bin_strtod, b"0b101\0"), (5.0, 5));
            assert_eq!(read(zend_bin_strtod, b"11\0"), (3.0, 2));
            assert_eq!(read(zend_oct_strtod, b"017\0"), (15.0, 3));
            assert_eq!(read(zend_oct_strtod, b"0\0"), (0.0, 0));
            assert_eq!(read(zend_oct_strtod, b"\0"), (0.0, 0));
        }
    }
}
