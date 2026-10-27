//! Purpose:
//! The string builders: `smart_str` (a growing `zend_string`) and
//! `smart_string` (a growing C buffer).
//!
//! Called from:
//! - Extension code (`smart_str_append*`, `smart_str_extract`...), and the
//!   engine's serializer and introspection.
//!
//! Key details:
//! - Growth follows PHP: the first allocation is 256 bytes less the header,
//!   later ones round up to a page. `ZSTR_LEN(s)` is the used length; the
//!   string is NUL-terminated only by `smart_str_0` or extraction.

use std::ffi::{c_char, c_void};
use std::ptr;

use crate::abi::*;
use crate::string::{self, zend_string_alloc, zend_string_release};

const HEADER: usize = std::mem::offset_of!(zend_string, val);
const PAGE: usize = 4096;
const START_LEN: usize = 256 - HEADER - 1;

/// The allocation capacity to grow a `smart_str` to for `len` bytes: the length plus header rounded
/// up to a page, less the header.
fn new_len(len: usize) -> usize {
    ((len + HEADER + 1 + PAGE - 1) & !(PAGE - 1)) - HEADER - 1
}

/// `smart_str_erealloc`: grows a `smart_str` so its buffer holds at least `len` bytes, allocating
/// the first buffer when none exists.
#[no_mangle]
pub unsafe extern "C" fn smart_str_erealloc(str: *mut smart_str, len: usize) {
    if (*str).s.is_null() {
        (*str).a = if len <= START_LEN { START_LEN } else { new_len(len) };
        (*str).s = zend_string_alloc((*str).a, false);
        (*(*str).s).len = 0;
    } else {
        (*str).a = new_len(len);
        let used = (*(*str).s).len;
        let grown = crate::alloc::erealloc((*str).s.cast(), HEADER + (*str).a + 1).cast::<zend_string>();
        (*grown).len = used;
        (*str).s = grown;
    }
}

/// `smart_str_realloc`: persistent spelling of `smart_str_erealloc`; both behave the same here.
#[no_mangle]
pub unsafe extern "C" fn smart_str_realloc(str: *mut smart_str, len: usize) {
    smart_str_erealloc(str, len);
}

/// Makes room for `len` more bytes; returns the new length.
#[no_mangle]
pub unsafe extern "C" fn smart_str_alloc(str: *mut smart_str, len: usize, _persistent: bool) -> usize {
    if (*str).s.is_null() {
        smart_str_erealloc(str, len);
        return len;
    }
    let total = len + (*(*str).s).len;
    if total >= (*str).a {
        smart_str_erealloc(str, total);
    }
    total
}

/// `smart_str_extend_ex`: reserves `len` more bytes at the end of the string, counts them in its
/// length, and returns a pointer to the reserved space for the caller to fill.
#[no_mangle]
pub unsafe extern "C" fn smart_str_extend_ex(dest: *mut smart_str, len: usize, persistent: bool) -> *mut c_char {
    let new_len = smart_str_alloc(dest, len, persistent);
    let at = (*(*dest).s).val.as_mut_ptr().add((*(*dest).s).len);
    (*(*dest).s).len = new_len;
    at
}

/// `smart_str_extend`: `smart_str_extend_ex` for a non-persistent string.
#[no_mangle]
pub unsafe extern "C" fn smart_str_extend(dest: *mut smart_str, len: usize) -> *mut c_char {
    smart_str_extend_ex(dest, len, false)
}

/// `smart_str_free_ex`: releases the string buffer and resets the pointer to NULL.
#[no_mangle]
pub unsafe extern "C" fn smart_str_free_ex(str: *mut smart_str, _persistent: bool) {
    if !(*str).s.is_null() {
        zend_string_release((*str).s);
        (*str).s = ptr::null_mut();
    }
    (*str).a = 0;
}

/// `smart_str_free`: releases a non-persistent `smart_str` buffer.
#[no_mangle]
pub unsafe extern "C" fn smart_str_free(str: *mut smart_str) {
    smart_str_free_ex(str, false);
}

/// `smart_str_0`: NUL-terminates the string (a no-op when nothing was allocated).
#[no_mangle]
pub unsafe extern "C" fn smart_str_0(str: *mut smart_str) {
    if !(*str).s.is_null() {
        *(*(*str).s).val.as_mut_ptr().add((*(*str).s).len) = 0;
    }
}

/// `smart_str_get_len`: the used length of the string, 0 when nothing was allocated.
#[no_mangle]
pub unsafe extern "C" fn smart_str_get_len(str: *const smart_str) -> usize {
    if (*str).s.is_null() {
        0
    } else {
        (*(*str).s).len
    }
}

/// `smart_str_trim_to_size_ex`: shrinks the buffer to exactly the used length.
#[no_mangle]
pub unsafe extern "C" fn smart_str_trim_to_size_ex(str: *mut smart_str, persistent: bool) {
    if !(*str).s.is_null() && (*str).a > (*(*str).s).len {
        (*str).s = string::zend_string_realloc((*str).s, (*(*str).s).len, persistent);
        (*str).a = (*(*str).s).len;
    }
}

/// `smart_str_trim_to_size`: `smart_str_trim_to_size_ex` for a non-persistent string.
#[no_mangle]
pub unsafe extern "C" fn smart_str_trim_to_size(str: *mut smart_str) {
    smart_str_trim_to_size_ex(str, false);
}

/// `smart_str_extract_ex`: NUL-terminates, trims and hands the accumulated `zend_string` to the
/// caller, leaving the builder empty (or returns the empty string when nothing was appended).
#[no_mangle]
pub unsafe extern "C" fn smart_str_extract_ex(str: *mut smart_str, persistent: bool) -> *mut zend_string {
    if (*str).s.is_null() {
        return string::zend_empty_string;
    }
    smart_str_0(str);
    smart_str_trim_to_size_ex(str, persistent);
    let result = (*str).s;
    (*str).s = ptr::null_mut();
    (*str).a = 0;
    result
}

/// `smart_str_extract`: `smart_str_extract_ex` for a non-persistent string.
#[no_mangle]
pub unsafe extern "C" fn smart_str_extract(str: *mut smart_str) -> *mut zend_string {
    smart_str_extract_ex(str, false)
}

/// `smart_str_appendc_ex`: appends one character.
#[no_mangle]
pub unsafe extern "C" fn smart_str_appendc_ex(dest: *mut smart_str, ch: c_char, persistent: bool) {
    let new_len = smart_str_alloc(dest, 1, persistent);
    *(*(*dest).s).val.as_mut_ptr().add(new_len - 1) = ch;
    (*(*dest).s).len = new_len;
}

/// `smart_str_appendl_ex`: appends `len` bytes from a pointer.
#[no_mangle]
pub unsafe extern "C" fn smart_str_appendl_ex(dest: *mut smart_str, str: *const c_char, len: usize, persistent: bool) {
    let new_len = smart_str_alloc(dest, len, persistent);
    if len > 0 {
        ptr::copy_nonoverlapping(str, (*(*dest).s).val.as_mut_ptr().add((*(*dest).s).len), len);
    }
    (*(*dest).s).len = new_len;
}

/// `smart_str_append_ex`: appends the bytes of a `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_ex(dest: *mut smart_str, src: *const zend_string, persistent: bool) {
    smart_str_appendl_ex(dest, (*src).val.as_ptr(), (*src).len, persistent);
}

/// `smart_str_append_smart_str_ex`: appends the contents of another `smart_str` (nothing when it is
/// empty).
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_smart_str_ex(dest: *mut smart_str, src: *const smart_str, persistent: bool) {
    if !(*src).s.is_null() && (*(*src).s).len > 0 {
        smart_str_append_ex(dest, (*src).s, persistent);
    }
}

/// `smart_str_setl`: replaces the contents with `len` bytes from a pointer.
#[no_mangle]
pub unsafe extern "C" fn smart_str_setl(dest: *mut smart_str, src: *const c_char, len: usize) {
    smart_str_free(dest);
    smart_str_appendl_ex(dest, src, len, false);
}

/// Appends Rust bytes.
pub unsafe fn append_bytes(dest: *mut smart_str, data: &[u8]) {
    smart_str_appendl_ex(dest, data.as_ptr().cast(), data.len(), false);
}

/// `smart_str_append_long`: appends an integer in decimal.
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_long(dest: *mut smart_str, num: zend_long) {
    append_bytes(dest, num.to_string().as_bytes());
}

/// `smart_str_append_unsigned`: appends an unsigned integer in decimal.
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_unsigned(dest: *mut smart_str, num: zend_ulong) {
    append_bytes(dest, num.to_string().as_bytes());
}

/// `smart_str_append_double`: appends a float in PHP's repr style with the given precision (0
/// counts as 1) and optional trailing `.0`.
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_double(dest: *mut smart_str, num: f64, precision: i32, zero_fraction: bool) {
    let precision = if precision == 0 { 1 } else { precision };
    append_bytes(dest, crate::convert::double_repr(num, precision, zero_fraction).as_bytes());
}

/// Appends `s` escaped the way PHP's debug output escapes it (`\n`, `\\`,
/// octal for other controls).
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_escaped(str: *mut smart_str, s: *const c_char, l: usize) {
    let bytes = if l == 0 { &[][..] } else { std::slice::from_raw_parts(s.cast::<u8>(), l) };
    let mut out = Vec::with_capacity(l);
    for &byte in bytes {
        match byte {
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x0b => out.extend_from_slice(b"\\v"),
            0x0c => out.extend_from_slice(b"\\f"),
            0x1b => out.extend_from_slice(b"\\e"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            c if c < 32 || c > 126 => out.extend_from_slice(format!("\\x{c:02X}").as_bytes()),
            c => out.push(c),
        }
    }
    append_bytes(str, &out);
}

/// `smart_str_append_escaped_truncated`: appends at most `length` bytes of a `zend_string` escaped
/// like `smart_str_append_escaped`, followed by `...` when the string was longer.
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_escaped_truncated(str: *mut smart_str, value: *const zend_string, length: usize) {
    smart_str_append_escaped(str, (*value).val.as_ptr(), (*value).len.min(length));
    if (*value).len > length {
        append_bytes(str, b"...");
    }
}

/// `smart_str_append_scalar`: appends a scalar as backtrace-style debug text (`NULL`, `true`,
/// `false`, numbers, and strings single-quoted, escaped and truncated at `truncate` bytes).
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_scalar(dest: *mut smart_str, value: *const zval, truncate: usize) {
    match (*value).u1.v.r#type {
        IS_UNDEF | IS_NULL => append_bytes(dest, b"NULL"),
        IS_TRUE => append_bytes(dest, b"true"),
        IS_FALSE => append_bytes(dest, b"false"),
        IS_DOUBLE => smart_str_append_double(dest, (*value).value.dval, -1, false),
        IS_LONG => smart_str_append_long(dest, (*value).value.lval),
        IS_STRING => {
            append_bytes(dest, b"'");
            smart_str_append_escaped_truncated(dest, (*value).value.str, truncate);
            append_bytes(dest, b"'");
        }
        _ => {}
    }
}

/// `smart_str_append_zval`: appends a zval as debug text: `Object(Class)`, `Array`, or the scalar
/// form.
#[no_mangle]
pub unsafe extern "C" fn smart_str_append_zval(dest: *mut smart_str, value: *const zval, truncate: usize) {
    match (*value).u1.v.r#type {
        IS_OBJECT => {
            append_bytes(dest, b"Object(");
            append_bytes(dest, string::bytes((*(*(*value).value.obj).ce).name));
            append_bytes(dest, b")");
        }
        IS_ARRAY => append_bytes(dest, b"Array"),
        _ => smart_str_append_scalar(dest, value, truncate),
    }
}

// ------------------------------------------------------------ smart_string

/// `_smart_string_alloc`: allocates or grows a `smart_string` buffer to fit `len` more bytes.
#[no_mangle]
pub unsafe extern "C" fn _smart_string_alloc(str: *mut smart_string, len: usize) {
    if (*str).c.is_null() {
        (*str).len = 0;
        (*str).a = if len <= 255 { 255 } else { len + 128 };
        (*str).c = crate::alloc::alloc((*str).a + 1).cast();
    } else {
        (*str).a = (*str).len + len + 128;
        (*str).c = crate::alloc::erealloc((*str).c.cast::<c_void>(), (*str).a + 1).cast();
    }
}

/// `_smart_string_alloc_persistent`: persistent spelling of `_smart_string_alloc`.
#[no_mangle]
pub unsafe extern "C" fn _smart_string_alloc_persistent(str: *mut smart_string, len: usize) {
    _smart_string_alloc(str, len);
}

/// `smart_string_alloc_repeated`: growth helper for repeated appends; same as
/// `_smart_string_alloc`.
#[no_mangle]
pub unsafe extern "C" fn smart_string_alloc_repeated(str: *mut smart_string, len: usize) {
    _smart_string_alloc(str, len);
}

/// `smart_string_alloc`: ensures a `smart_string` has room for `len` more bytes, growing it only
/// when needed.
#[no_mangle]
pub unsafe extern "C" fn smart_string_alloc(str: *mut smart_string, len: usize, _persistent: bool) {
    if (*str).c.is_null() || len >= (*str).a - (*str).len {
        _smart_string_alloc(str, len);
    }
}

/// `smart_string_free_ex`: frees the buffer and resets the pointer to NULL.
#[no_mangle]
pub unsafe extern "C" fn smart_string_free_ex(str: *mut smart_string, _persistent: bool) {
    if !(*str).c.is_null() {
        crate::alloc::free((*str).c.cast());
        (*str).c = ptr::null_mut();
    }
    (*str).a = 0;
    (*str).len = 0;
}

/// `smart_string_0`: NUL-terminates the string (a no-op when nothing was allocated).
#[no_mangle]
pub unsafe extern "C" fn smart_string_0(str: *mut smart_string) {
    if !(*str).c.is_null() {
        *(*str).c.add((*str).len) = 0;
    }
}

/// `smart_string_appendc_ex`: appends one character to a `smart_string`.
#[no_mangle]
pub unsafe extern "C" fn smart_string_appendc_ex(dest: *mut smart_string, ch: c_char, persistent: bool) {
    smart_string_alloc(dest, 1, persistent);
    *(*dest).c.add((*dest).len) = ch;
    (*dest).len += 1;
}

/// `smart_string_appendl_ex`: appends `len` bytes from a pointer to a `smart_string`.
#[no_mangle]
pub unsafe extern "C" fn smart_string_appendl_ex(dest: *mut smart_string, str: *const c_char, len: usize, persistent: bool) {
    smart_string_alloc(dest, len, persistent);
    if len > 0 {
        ptr::copy_nonoverlapping(str, (*dest).c.add((*dest).len), len);
    }
    (*dest).len += len;
}

/// `smart_string_append_long_ex`: appends an integer in decimal to a `smart_string`.
#[no_mangle]
pub unsafe extern "C" fn smart_string_append_long_ex(dest: *mut smart_string, num: zend_long, persistent: bool) {
    let text = num.to_string();
    smart_string_appendl_ex(dest, text.as_ptr().cast(), text.len(), persistent);
}

/// `smart_string_append_unsigned_ex`: appends an unsigned integer in decimal to a `smart_string`.
#[no_mangle]
pub unsafe extern "C" fn smart_string_append_unsigned_ex(dest: *mut smart_string, num: zend_ulong, persistent: bool) {
    let text = num.to_string();
    smart_string_appendl_ex(dest, text.as_ptr().cast(), text.len(), persistent);
}
