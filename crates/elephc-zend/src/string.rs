//! Purpose:
//! `zend_string`: allocation, refcounting, hashing, interning, comparison and
//! case folding, plus the empty, one-character and known strings.
//!
//! Called from:
//! - Extension code, and every engine module that names something.
//!
//! Key details:
//! - Layout is PHP's: a refcounted header, the cached hash, the length, then
//!   the bytes and a NUL. `ZSTR_VAL` and `ZSTR_LEN` are plain field reads.
//! - The hash is PHP's DJBX33A with the top bit set, so it is never zero (zero
//!   means "not computed yet") and tables built here hash like PHP's.
//! - Interned strings are never freed: the interning table owns them, and
//!   refcounting skips them (`IS_STR_INTERNED`).

use std::collections::HashMap;
use std::ffi::{c_char, c_int};
use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc, free};
use crate::global::Global;

const HEADER: usize = std::mem::offset_of!(zend_string, val);

/// Texts of `zend_known_strings`, indexed by `ZEND_STR_*`.
pub const KNOWN_STRINGS: &[&str] = &[
    "file", "line", "function", "class", "object", "type", "->", "::", "args", "unknown", "Unknown", "eval",
    "include", "require", "include_once", "require_once", "scalar", "error_reporting", "static", "this", "value",
    "key", "__invoke", "previous", "code", "message", "severity", "string", "trace", "name", "argv", "argc",
    "Array", "bool", "int", "float", "callable", "iterable", "void", "never", "false", "true", "null", "mixed",
    "Traversable", "self", "parent", "array", "NULL", "double", "integer", "__sleep", "__wakeup", "cases", "from",
    "tryFrom", "tryfrom", "_SERVER", "_ENV", "_REQUEST", "count", "SensitiveParameter", "[constant expression]",
    "Deprecated", "since", "get", "set", "8.0", "8.1", "8.2", "8.3", "8.4", "8.5",
];

#[no_mangle]
pub static mut zend_empty_string: *mut zend_string = ptr::null_mut();
#[no_mangle]
pub static mut zend_one_char_string: [*mut zend_string; 256] = [ptr::null_mut(); 256];
#[no_mangle]
pub static mut zend_known_strings: *mut *mut zend_string = ptr::null_mut();

static INTERNED: Global<Option<HashMap<Vec<u8>, *mut zend_string>>> = Global::new(None);

/// Builds the interned strings every extension may use. Idempotent.
pub fn startup() {
    unsafe {
        if !zend_empty_string.is_null() {
            return;
        }
        zend_empty_string = intern(b"");
        for byte in 0..=255u8 {
            zend_one_char_string[byte as usize] = intern(&[byte]);
        }
        let known: Vec<*mut zend_string> = KNOWN_STRINGS.iter().map(|text| intern(text.as_bytes())).collect();
        zend_known_strings = Box::leak(known.into_boxed_slice()).as_mut_ptr();
    }
}

/// The bytes of a string.
///
/// # Safety
/// `s` must be a live zend_string.
pub unsafe fn bytes<'a>(s: *const zend_string) -> &'a [u8] {
    std::slice::from_raw_parts((*s).val.as_ptr().cast::<u8>(), (*s).len)
}

/// The bytes as text, lossily (for messages).
pub unsafe fn text(s: *const zend_string) -> String {
    String::from_utf8_lossy(bytes(s)).into_owned()
}

/// Returns the GC flag bits (type info masked by `GC_FLAGS_MASK`) of a string.
pub unsafe fn flags(s: *const zend_string) -> u32 {
    (*s).gc.u.type_info & GC_FLAGS_MASK
}

/// True when the string is interned (never freed or reference counted).
pub unsafe fn is_interned(s: *const zend_string) -> bool {
    flags(s) & IS_STR_INTERNED != 0
}

/// A new string holding `data`, refcount 1.
pub fn new(data: &[u8]) -> *mut zend_string {
    unsafe {
        let s = zend_string_alloc(data.len(), false);
        ptr::copy_nonoverlapping(data.as_ptr(), (*s).val.as_mut_ptr().cast::<u8>(), data.len());
        s
    }
}

/// The interned string for `data`.
pub fn intern(data: &[u8]) -> *mut zend_string {
    unsafe {
        let table = INTERNED.get().get_or_insert_with(HashMap::new);
        if let Some(existing) = table.get(data) {
            return *existing;
        }
        let s = new(data);
        (*s).gc.u.type_info = GC_STRING | IS_STR_INTERNED | IS_STR_PERMANENT;
        zend_string_hash_val(s);
        table.insert(data.to_vec(), s);
        s
    }
}

/// Returns the engine's known string number `id` from the `zend_known_strings` table.
pub fn known(id: c_int) -> *mut zend_string {
    unsafe { *zend_known_strings.add(id as usize) }
}

/// DJBX33A, PHP's string hash, with the top bit set.
pub fn hash_bytes(data: &[u8]) -> zend_ulong {
    let mut hash: u64 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash | 0x8000000000000000
}

/// Allocates an uninitialized string of `len` bytes (header, bytes and trailing NUL, 8-byte
/// aligned) with refcount 1, hash 0 and the persistent flag if requested.
#[no_mangle]
pub extern "C" fn zend_string_alloc(len: usize, persistent: bool) -> *mut zend_string {
    let size = (HEADER + len + 1 + 7) & !7;
    let s = alloc(size).cast::<zend_string>();
    unsafe {
        (*s).gc.refcount = 1;
        (*s).gc.u.type_info = GC_STRING | if persistent { IS_STR_PERSISTENT } else { 0 };
        (*s).h = 0;
        (*s).len = len;
        *(*s).val.as_mut_ptr().add(len) = 0;
    }
    s
}

/// Allocates a string of `n * m + l` bytes, raising a fatal "integer overflow" error when that size
/// overflows.
#[no_mangle]
pub extern "C" fn zend_string_safe_alloc(n: usize, m: usize, l: usize, persistent: bool) -> *mut zend_string {
    let len = n
        .checked_mul(m)
        .and_then(|v| v.checked_add(l))
        .unwrap_or_else(|| crate::error::fatal(&format!("Possible integer overflow in memory allocation ({n} * {m} + {l})")));
    zend_string_alloc(len, persistent)
}

/// Allocates a string and copies `len` bytes of `str` into it (refcount 1).
#[no_mangle]
pub unsafe extern "C" fn zend_string_init(str: *const c_char, len: usize, persistent: bool) -> *mut zend_string {
    let s = zend_string_alloc(len, persistent);
    if len > 0 {
        ptr::copy_nonoverlapping(str, (*s).val.as_mut_ptr(), len);
    }
    s
}

/// Like `zend_string_init` but returns the shared empty string for length 0 and the shared
/// one-character string for length 1.
#[no_mangle]
pub unsafe extern "C" fn zend_string_init_fast(str: *const c_char, len: usize) -> *mut zend_string {
    match len {
        0 => zend_empty_string,
        1 => zend_one_char_string[*str as u8 as usize],
        _ => zend_string_init(str, len, false),
    }
}

/// Builds a byte slice from a C pointer and length, returning an empty slice for length 0 so a null
/// pointer is never dereferenced.
unsafe fn slice<'a>(str: *const c_char, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(str.cast::<u8>(), len)
    }
}

/// Returns the interned string for the given bytes (the permanence flag is ignored; every interned
/// string lives forever).
#[no_mangle]
pub unsafe extern "C" fn zend_string_init_interned(str: *const c_char, len: usize, _permanent: bool) -> *mut zend_string {
    intern(slice(str, len))
}

/// Returns the interned string for the given bytes, creating it if absent (the permanence flag is
/// ignored).
#[no_mangle]
pub unsafe extern "C" fn zend_string_init_existing_interned(
    str: *const c_char,
    len: usize,
    _permanent: bool,
) -> *mut zend_string {
    intern(slice(str, len))
}

/// Replaces a string with its interned equivalent, releasing the caller's reference to the
/// original; an already-interned string is returned unchanged.
#[no_mangle]
pub unsafe extern "C" fn zend_new_interned_string(str: *mut zend_string) -> *mut zend_string {
    if is_interned(str) {
        return str;
    }
    let interned = intern(bytes(str));
    zend_string_release(str);
    interned
}

/// Looks up an existing interned string with the same bytes; returns null when none exists.
#[no_mangle]
pub unsafe extern "C" fn zend_interned_string_find_permanent(str: *mut zend_string) -> *mut zend_string {
    let table = INTERNED.get().get_or_insert_with(HashMap::new);
    table.get(bytes(str)).copied().unwrap_or(ptr::null_mut())
}

/// Returns the same string with one more reference (interned strings are returned as is).
#[no_mangle]
pub unsafe extern "C" fn zend_string_copy(s: *mut zend_string) -> *mut zend_string {
    if !is_interned(s) {
        (*s).gc.refcount += 1;
    }
    s
}

/// Returns a new copy of a non-interned string, or the string itself when interned.
#[no_mangle]
pub unsafe extern "C" fn zend_string_dup(s: *mut zend_string, persistent: bool) -> *mut zend_string {
    if is_interned(s) {
        return s;
    }
    zend_string_init((*s).val.as_ptr(), (*s).len, persistent)
}

/// Returns a string the caller may modify: the same one with its hash cleared when uniquely
/// referenced, otherwise a fresh copy (dropping the caller's reference to the original).
#[no_mangle]
pub unsafe extern "C" fn zend_string_separate(s: *mut zend_string, persistent: bool) -> *mut zend_string {
    if is_interned(s) || (*s).gc.refcount > 1 {
        if !is_interned(s) {
            (*s).gc.refcount -= 1;
        }
        return zend_string_init((*s).val.as_ptr(), (*s).len, persistent);
    }
    (*s).h = 0;
    s
}

/// Resizes a string to `len` bytes: in place when uniquely referenced and not interned, otherwise
/// into a new copy of the common prefix (dropping the caller's reference). The hash is cleared.
#[no_mangle]
pub unsafe extern "C" fn zend_string_realloc(s: *mut zend_string, len: usize, persistent: bool) -> *mut zend_string {
    if !is_interned(s) && (*s).gc.refcount == 1 {
        let size = (HEADER + len + 1 + 7) & !7;
        let grown = crate::alloc::erealloc(s.cast(), size).cast::<zend_string>();
        (*grown).len = len;
        (*grown).h = 0;
        *(*grown).val.as_mut_ptr().add(len) = 0;
        return grown;
    }
    let copy = zend_string_alloc(len, persistent);
    ptr::copy_nonoverlapping((*s).val.as_ptr(), (*copy).val.as_mut_ptr(), len.min((*s).len));
    if !is_interned(s) {
        (*s).gc.refcount -= 1;
    }
    copy
}

/// Grows a string to `len` bytes; same as `zend_string_realloc`.
#[no_mangle]
pub unsafe extern "C" fn zend_string_extend(s: *mut zend_string, len: usize, persistent: bool) -> *mut zend_string {
    zend_string_realloc(s, len, persistent)
}

/// Shrinks a string to `len` bytes; same as `zend_string_realloc`.
#[no_mangle]
pub unsafe extern "C" fn zend_string_truncate(s: *mut zend_string, len: usize, persistent: bool) -> *mut zend_string {
    zend_string_realloc(s, len, persistent)
}

/// Resizes a string to `n * m + l` bytes, raising a fatal "integer overflow" error when that size
/// overflows.
#[no_mangle]
pub unsafe extern "C" fn zend_string_safe_realloc(
    s: *mut zend_string,
    n: usize,
    m: usize,
    l: usize,
    persistent: bool,
) -> *mut zend_string {
    let len = n
        .checked_mul(m)
        .and_then(|v| v.checked_add(l))
        .unwrap_or_else(|| crate::error::fatal(&format!("Possible integer overflow in memory allocation ({n} * {m} + {l})")));
    zend_string_realloc(s, len, persistent)
}

/// Frees a non-interned string's memory regardless of its refcount.
#[no_mangle]
pub unsafe extern "C" fn zend_string_free(s: *mut zend_string) {
    if !is_interned(s) {
        free(s.cast());
    }
}

/// Frees a string's memory unconditionally.
#[no_mangle]
pub unsafe extern "C" fn zend_string_efree(s: *mut zend_string) {
    free(s.cast());
}

/// Drops one reference to a non-interned string and frees it at zero; interned strings are left
/// alone.
#[no_mangle]
pub unsafe extern "C" fn zend_string_release(s: *mut zend_string) {
    if !is_interned(s) {
        (*s).gc.refcount -= 1;
        if (*s).gc.refcount == 0 {
            free(s.cast());
        }
    }
}

/// Same as `zend_string_release`; the persistence argument is ignored.
#[no_mangle]
pub unsafe extern "C" fn zend_string_release_ex(s: *mut zend_string, _persistent: bool) {
    zend_string_release(s);
}

/// Returns the reference count, reporting 1 for interned strings.
#[no_mangle]
pub unsafe extern "C" fn zend_string_refcount(s: *const zend_string) -> u32 {
    if is_interned(s) {
        1
    } else {
        (*s).gc.refcount
    }
}

/// Increments the reference count of a non-interned string and returns the new count (1 for
/// interned strings).
#[no_mangle]
pub unsafe extern "C" fn zend_string_addref(s: *mut zend_string) -> u32 {
    if is_interned(s) {
        return 1;
    }
    (*s).gc.refcount += 1;
    (*s).gc.refcount
}

/// Decrements the reference count of a non-interned string without freeing and returns the new
/// count (1 for interned strings).
#[no_mangle]
pub unsafe extern "C" fn zend_string_delref(s: *mut zend_string) -> u32 {
    if is_interned(s) {
        return 1;
    }
    (*s).gc.refcount -= 1;
    (*s).gc.refcount
}

/// Returns the string's DJBX33A hash, computing and caching it on first use.
#[no_mangle]
pub unsafe extern "C" fn zend_string_hash_val(s: *mut zend_string) -> zend_ulong {
    if (*s).h == 0 {
        (*s).h = hash_bytes(bytes(s));
    }
    (*s).h
}

/// Recomputes, stores and returns the string's hash.
#[no_mangle]
pub unsafe extern "C" fn zend_string_hash_func(s: *mut zend_string) -> zend_ulong {
    (*s).h = hash_bytes(bytes(s));
    (*s).h
}

/// Computes the PHP string hash of a C buffer.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_func(str: *const c_char, len: usize) -> zend_ulong {
    hash_bytes(slice(str, len))
}

/// Computes the PHP string hash of a C buffer (same as `zend_hash_func`).
#[no_mangle]
pub unsafe extern "C" fn zend_inline_hash_func(str: *const c_char, len: usize) -> zend_ulong {
    hash_bytes(slice(str, len))
}

/// Clears the cached hash so it is recomputed on next use (call after mutating the bytes).
#[no_mangle]
pub unsafe extern "C" fn zend_string_forget_hash_val(s: *mut zend_string) {
    (*s).h = 0;
}

/// True when two strings are the same object or have identical bytes.
#[no_mangle]
pub unsafe extern "C" fn zend_string_equals(s1: *const zend_string, s2: *const zend_string) -> bool {
    s1 == s2 || bytes(s1) == bytes(s2)
}

/// True when two strings have identical bytes.
#[no_mangle]
pub unsafe extern "C" fn zend_string_equal_content(s1: *const zend_string, s2: *const zend_string) -> bool {
    bytes(s1) == bytes(s2)
}

/// True when two strings are equal ignoring ASCII case.
#[no_mangle]
pub unsafe extern "C" fn zend_string_equals_ci(s1: *const zend_string, s2: *const zend_string) -> bool {
    bytes(s1).eq_ignore_ascii_case(bytes(s2))
}

/// True when the string's bytes equal the given buffer.
#[no_mangle]
pub unsafe extern "C" fn zend_string_equals_cstr(s1: *const zend_string, s2: *const c_char, s2_length: usize) -> bool {
    bytes(s1) == slice(s2, s2_length)
}

/// True when the string equals the given buffer ignoring ASCII case.
#[no_mangle]
pub unsafe extern "C" fn zend_string_equals_cstr_ci(
    s1: *const zend_string,
    s2: *const c_char,
    s2_length: usize,
) -> bool {
    bytes(s1).eq_ignore_ascii_case(slice(s2, s2_length))
}

/// True when the string begins with the given buffer.
#[no_mangle]
pub unsafe extern "C" fn zend_string_starts_with_cstr(
    str: *const zend_string,
    prefix: *const c_char,
    prefix_length: usize,
) -> bool {
    bytes(str).starts_with(slice(prefix, prefix_length))
}

/// True when the string begins with the given buffer, ignoring ASCII case.
#[no_mangle]
pub unsafe extern "C" fn zend_string_starts_with_cstr_ci(
    str: *const zend_string,
    prefix: *const c_char,
    prefix_length: usize,
) -> bool {
    let text = bytes(str);
    let prefix = slice(prefix, prefix_length);
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// True when the string begins with the bytes of another `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn zend_string_starts_with(str: *const zend_string, prefix: *const zend_string) -> bool {
    bytes(str).starts_with(bytes(prefix))
}

/// A lowercased copy, or the string itself (one more reference) when it has
/// no uppercase ASCII byte.
#[no_mangle]
pub unsafe extern "C" fn zend_string_tolower_ex(str: *mut zend_string, persistent: bool) -> *mut zend_string {
    let text = bytes(str);
    if !text.iter().any(u8::is_ascii_uppercase) {
        return zend_string_copy(str);
    }
    let lower = zend_string_alloc(text.len(), persistent);
    for (i, byte) in text.iter().enumerate() {
        *(*lower).val.as_mut_ptr().add(i) = byte.to_ascii_lowercase() as c_char;
    }
    lower
}

/// An ASCII-uppercased copy, or the string itself (one more reference) when it has no lowercase
/// ASCII byte; `persistent` applies to a new copy.
#[no_mangle]
pub unsafe extern "C" fn zend_string_toupper_ex(str: *mut zend_string, persistent: bool) -> *mut zend_string {
    let text = bytes(str);
    if !text.iter().any(u8::is_ascii_lowercase) {
        return zend_string_copy(str);
    }
    let upper = zend_string_alloc(text.len(), persistent);
    for (i, byte) in text.iter().enumerate() {
        *(*upper).val.as_mut_ptr().add(i) = byte.to_ascii_uppercase() as c_char;
    }
    upper
}

/// Lowercase variant of the string with a non-persistent copy; see `zend_string_tolower_ex`.
#[no_mangle]
pub unsafe extern "C" fn zend_string_tolower(str: *mut zend_string) -> *mut zend_string {
    zend_string_tolower_ex(str, false)
}

/// Uppercase variant of the string with a non-persistent copy; see `zend_string_toupper_ex`.
#[no_mangle]
pub unsafe extern "C" fn zend_string_toupper(str: *mut zend_string) -> *mut zend_string {
    zend_string_toupper_ex(str, false)
}

/// Returns a new string holding the two buffers joined.
#[no_mangle]
pub unsafe extern "C" fn zend_string_concat2(
    str1: *const c_char,
    str1_len: usize,
    str2: *const c_char,
    str2_len: usize,
) -> *mut zend_string {
    let mut joined = slice(str1, str1_len).to_vec();
    joined.extend_from_slice(slice(str2, str2_len));
    new(&joined)
}

/// Returns a new string holding the three buffers joined.
#[no_mangle]
pub unsafe extern "C" fn zend_string_concat3(
    str1: *const c_char,
    str1_len: usize,
    str2: *const c_char,
    str2_len: usize,
    str3: *const c_char,
    str3_len: usize,
) -> *mut zend_string {
    let mut joined = slice(str1, str1_len).to_vec();
    joined.extend_from_slice(slice(str2, str2_len));
    joined.extend_from_slice(slice(str3, str3_len));
    new(&joined)
}

/// ASCII-lowercases `length` bytes in place.
#[no_mangle]
pub unsafe extern "C" fn zend_str_tolower(str: *mut c_char, length: usize) {
    for i in 0..length {
        let byte = str.add(i);
        *byte = (*byte as u8).to_ascii_lowercase() as c_char;
    }
}

/// ASCII-uppercases `length` bytes in place.
#[no_mangle]
pub unsafe extern "C" fn zend_str_toupper(str: *mut c_char, length: usize) {
    for i in 0..length {
        let byte = str.add(i);
        *byte = (*byte as u8).to_ascii_uppercase() as c_char;
    }
}

/// Writes an ASCII-lowercased copy of `source` into `dest` and NUL-terminates it (`dest` needs
/// `length + 1` bytes); returns `dest`.
#[no_mangle]
pub unsafe extern "C" fn zend_str_tolower_copy(dest: *mut c_char, source: *const c_char, length: usize) -> *mut c_char {
    for i in 0..length {
        *dest.add(i) = (*source.add(i) as u8).to_ascii_lowercase() as c_char;
    }
    *dest.add(length) = 0;
    dest
}

/// Writes an ASCII-uppercased copy of `source` into `dest` and NUL-terminates it (`dest` needs
/// `length + 1` bytes); returns `dest`.
#[no_mangle]
pub unsafe extern "C" fn zend_str_toupper_copy(dest: *mut c_char, source: *const c_char, length: usize) -> *mut c_char {
    for i in 0..length {
        *dest.add(i) = (*source.add(i) as u8).to_ascii_uppercase() as c_char;
    }
    *dest.add(length) = 0;
    dest
}

/// Returns a newly allocated NUL-terminated lowercase copy of the buffer (the caller frees it).
#[no_mangle]
pub unsafe extern "C" fn zend_str_tolower_dup(source: *const c_char, length: usize) -> *mut c_char {
    zend_str_tolower_copy(alloc(length + 1).cast(), source, length)
}

/// Returns a lowercase copy only when the buffer contains an uppercase ASCII byte, otherwise null.
#[no_mangle]
pub unsafe extern "C" fn zend_str_tolower_dup_ex(source: *const c_char, length: usize) -> *mut c_char {
    if slice(source, length).iter().any(u8::is_ascii_uppercase) {
        zend_str_tolower_dup(source, length)
    } else {
        ptr::null_mut()
    }
}

/// Returns a newly allocated NUL-terminated uppercase copy of the buffer (the caller frees it).
#[no_mangle]
pub unsafe extern "C" fn zend_str_toupper_dup(source: *const c_char, length: usize) -> *mut c_char {
    zend_str_toupper_copy(alloc(length + 1).cast(), source, length)
}

/// Compares two lengths and returns -1, 0 or 1.
fn three_way(a: usize, b: usize) -> c_int {
    match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Compares two byte strings like `memcmp` then length: the difference of the first differing
/// bytes, otherwise -1, 0 or 1 by length.
fn byte_compare(a: &[u8], b: &[u8]) -> c_int {
    let common = a.len().min(b.len());
    for i in 0..common {
        if a[i] != b[i] {
            return a[i] as c_int - b[i] as c_int;
        }
    }
    three_way(a.len(), b.len())
}

/// Like `byte_compare` but comparing ASCII-lowercased bytes.
fn lower_compare(a: &[u8], b: &[u8]) -> c_int {
    let common = a.len().min(b.len());
    for i in 0..common {
        let (x, y) = (a[i].to_ascii_lowercase(), b[i].to_ascii_lowercase());
        if x != y {
            return x as c_int - y as c_int;
        }
    }
    three_way(a.len(), b.len())
}

/// Binary-safe string comparison of two buffers; negative, zero or positive.
#[no_mangle]
pub unsafe extern "C" fn zend_binary_strcmp(s1: *const c_char, len1: usize, s2: *const c_char, len2: usize) -> c_int {
    byte_compare(slice(s1, len1), slice(s2, len2))
}

/// Binary-safe comparison of at most `length` bytes of each buffer.
#[no_mangle]
pub unsafe extern "C" fn zend_binary_strncmp(
    s1: *const c_char,
    len1: usize,
    s2: *const c_char,
    len2: usize,
    length: usize,
) -> c_int {
    byte_compare(slice(s1, len1.min(length)), slice(s2, len2.min(length)))
}

/// Binary-safe ASCII case-insensitive comparison of two buffers.
#[no_mangle]
pub unsafe extern "C" fn zend_binary_strcasecmp(s1: *const c_char, len1: usize, s2: *const c_char, len2: usize) -> c_int {
    lower_compare(slice(s1, len1), slice(s2, len2))
}

/// Binary-safe ASCII case-insensitive comparison of at most `length` bytes of each buffer.
#[no_mangle]
pub unsafe extern "C" fn zend_binary_strncasecmp(
    s1: *const c_char,
    len1: usize,
    s2: *const c_char,
    len2: usize,
    length: usize,
) -> c_int {
    lower_compare(slice(s1, len1.min(length)), slice(s2, len2.min(length)))
}

/// Binary-safe comparison of the strings held by two string zvals.
#[no_mangle]
pub unsafe extern "C" fn zend_binary_zval_strcmp(s1: *mut zval, s2: *mut zval) -> c_int {
    byte_compare(bytes((*s1).value.str), bytes((*s2).value.str))
}

/// Finds the first occurrence of `needle` in `[haystack, end)`; returns a pointer to it, null if
/// absent, or `haystack` for an empty needle.
#[no_mangle]
pub unsafe extern "C" fn zend_memnstr(
    haystack: *const c_char,
    needle: *const c_char,
    needle_len: usize,
    end: *const c_char,
) -> *const c_char {
    let hay = slice(haystack, end.offset_from(haystack).max(0) as usize);
    let needle = slice(needle, needle_len);
    if needle.is_empty() {
        return haystack;
    }
    hay.windows(needle.len())
        .position(|window| window == needle)
        .map_or(ptr::null(), |at| haystack.add(at))
}

/// Finds the last occurrence of `needle` in `[haystack, end)`; returns a pointer to it, null if
/// absent, or `end` for an empty needle.
#[no_mangle]
pub unsafe extern "C" fn zend_memnrstr(
    haystack: *const c_char,
    needle: *const c_char,
    needle_len: usize,
    end: *const c_char,
) -> *const c_char {
    let hay = slice(haystack, end.offset_from(haystack).max(0) as usize);
    let needle = slice(needle, needle_len);
    if needle.is_empty() {
        return end;
    }
    hay.windows(needle.len())
        .rposition(|window| window == needle)
        .map_or(ptr::null(), |at| haystack.add(at))
}

/// Converts an integer to a new decimal string (single digits reuse the shared one-character
/// strings).
#[no_mangle]
pub extern "C" fn zend_long_to_str(num: zend_long) -> *mut zend_string {
    if (0..=9).contains(&num) {
        return unsafe { zend_one_char_string[(b'0' + num as u8) as usize] };
    }
    new(num.to_string().as_bytes())
}

/// Converts an unsigned integer to a new decimal string.
#[no_mangle]
pub extern "C" fn zend_ulong_to_str(num: zend_ulong) -> *mut zend_string {
    new(num.to_string().as_bytes())
}

/// Converts a `u64` to a new decimal string.
#[no_mangle]
pub extern "C" fn zend_u64_to_str(num: u64) -> *mut zend_string {
    new(num.to_string().as_bytes())
}

/// Converts an `i64` to a new decimal string.
#[no_mangle]
pub extern "C" fn zend_i64_to_str(num: i64) -> *mut zend_string {
    new(num.to_string().as_bytes())
}

/// Converts a float to a new string with 14 significant digits (PHP's default precision).
#[no_mangle]
pub extern "C" fn zend_double_to_str(num: f64) -> *mut zend_string {
    new(crate::convert::double_to_php(num, 14).as_bytes())
}

/// Writes `num` so that it ends at `buf` (where a NUL is stored) and returns
/// where it starts.
#[no_mangle]
pub unsafe extern "C" fn zend_print_long_to_buf(buf: *mut c_char, num: zend_long) -> *mut c_char {
    write_backwards(buf, num.to_string().as_bytes())
}

/// Writes an unsigned integer's decimal digits so they end at `buf` (a NUL is stored there) and
/// returns where they start.
#[no_mangle]
pub unsafe extern "C" fn zend_print_ulong_to_buf(buf: *mut c_char, num: zend_ulong) -> *mut c_char {
    write_backwards(buf, num.to_string().as_bytes())
}

/// Stores a NUL at `end`, copies `digits` immediately before it, and returns the start of the
/// digits.
unsafe fn write_backwards(end: *mut c_char, digits: &[u8]) -> *mut c_char {
    *end = 0;
    let start = end.sub(digits.len());
    ptr::copy_nonoverlapping(digits.as_ptr().cast::<c_char>(), start, digits.len());
    start
}

/// `zend_memnstr`, ignoring ASCII case.
#[no_mangle]
pub unsafe extern "C" fn zend_memnistr(
    haystack: *const c_char,
    needle: *const c_char,
    needle_len: usize,
    end: *const c_char,
) -> *const c_char {
    let hay = slice(haystack, end.offset_from(haystack).max(0) as usize);
    let needle = slice(needle, needle_len);
    if needle.is_empty() {
        return haystack;
    }
    hay.windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
        .map_or(ptr::null(), |at| haystack.add(at))
}
