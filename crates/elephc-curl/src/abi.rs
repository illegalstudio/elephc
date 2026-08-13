//! Purpose:
//! The Task 3 `elephc_curl_*` C ABI: the ten normative entry points
//! (`.superpowers/sdd/php-curl-family/task-3-brief.md`) that own easy-handle
//! lifecycle, URL/RETURNTRANSFER setup, performing a transfer, and reading
//! back the error/body/global-version results, plus one Task 7 addition
//! (`elephc_curl_easy_getinfo_long`, backing `curl_getinfo(..., CURLINFO_HTTP_CODE)` —
//! Task 3's brief explicitly deferred every info entry point to a later task). Every
//! function here is thin: it validates arguments, takes the table lock, and delegates to
//! `crate::easy` (raw libcurl) or `crate::php_layer` (PHP-only semantics).
//!
//! Called from:
//! - The PHP-program linker once a program uses curl (Task 4+); today, only
//!   `crates/elephc-curl/src/tests.rs`.
//!
//! Key details:
//! - Every entry point routes its whole body through
//!   `handles::ffi_guard` so a Rust panic never unwinds across the `extern
//!   "C"` boundary (undefined behavior) — it becomes the documented fallback
//!   value instead.
//! - Return-code convention: every `int32_t` "did it work" return here is a
//!   plain boolean (`1` success, `0` failure), with exactly one deliberate
//!   exception — `elephc_curl_easy_errno`, which returns the raw `CURLcode`
//!   by design (it exists specifically to back PHP's `curl_errno()`).
//! - `elephc_curl_easy_take_body`/`elephc_curl_easy_error`/
//!   `elephc_curl_global_info` write into caller-owned buffers and always
//!   report the required/actual length through their `*_len` out-parameter,
//!   even on a `0` (too-small-buffer) return — mirroring
//!   `elephc-crypto`'s cipher ABI (`crates/elephc-crypto/src/cipher/abi.rs`).
//! - **Caller contract — no concurrent calls on the same id.** The table
//!   mutex (`crate::handles::handles`) only serializes access to the table
//!   itself; `elephc_curl_easy_perform` and `elephc_curl_easy_free` both
//!   deliberately drop that lock before calling into libcurl (see their own
//!   doc comments below for why), so it does NOT stop a `free(id)` on one
//!   thread from running `curl_easy_cleanup` on the same `*mut CURL` while
//!   another thread is still blocked inside `perform(id)` — a real
//!   use-after-free at the libcurl level. This is sound only because
//!   elephc-compiled PHP programs are effectively single-threaded today, so
//!   no two `elephc_curl_*` calls for the same id are ever concurrent. Any
//!   future concurrent caller (a multi-threaded driver, or Task 9's multi
//!   interface if it ever fans work across OS threads) MUST itself guarantee
//!   that no two threads call into this ABI for the same id at once — see
//!   `crate::handles::EasyEntry`'s `Send` impl for the full reasoning.

use std::ffi::{c_char, c_int, c_void, CString};

use crate::easy;
use crate::handles::{self, EasyEntry};
use crate::options;
use crate::php_layer;

/// Returns the `elephc_curl` ABI version. v1 = the Task 3 surface described
/// in this module; bumped when the C ABI shape changes.
#[no_mangle]
pub extern "C" fn elephc_curl_version_abi() -> i32 {
    1
}

/// Classifies a `curl_setopt()` option number: which setter (if any) can carry
/// its value. See `crate::options` for the kind codes and for why this
/// classification is a memory-safety boundary rather than a convenience.
///
/// This is the ONE curl entry point that touches no handle and no libcurl
/// state: it is a pure lookup in the frozen option table, so the curl prelude
/// can ask it before it has decided which setter to call.
///
/// `opt` is an `int64_t`, not an `int32_t`, ON PURPOSE: PHP integers are
/// 64-bit, and a 32-bit parameter would let `curl_setopt($ch, 4294967298, …)`
/// truncate onto option `2` and be classified as a real option. Anything
/// outside `i32`'s range is simply not a cURL option, so it answers
/// [`options::KIND_INVALID`] and the prelude raises php-src's `ValueError`.
#[no_mangle]
pub extern "C" fn elephc_curl_option_kind(opt: i64) -> i32 {
    handles::ffi_guard(options::KIND_INVALID, || {
        i32::try_from(opt).map_or(options::KIND_INVALID, options::option_kind)
    })
}

/// Allocates a new libcurl easy handle, installs the write callback (see
/// `crate::php_layer::install_write_callback`) and error buffer, and
/// registers it in the handle table under a fresh id. Returns `0` on libcurl
/// allocation failure.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_init() -> i64 {
    handles::ffi_guard(0, || {
        let curl = easy::init();
        if curl.is_null() {
            return 0;
        }
        let id = handles::next_id();
        let mut entry = EasyEntry::new(curl);
        unsafe {
            php_layer::install_write_callback(curl, id);
            // `error_buf`'s heap allocation address is stable across the
            // upcoming move into the handle table (moving a `Vec` only moves
            // its {ptr,len,cap} header, never reallocates), so handing
            // libcurl this pointer now and inserting `entry` afterward is
            // sound as long as `error_buf` is never resized (see
            // `EasyEntry::error_buf`'s docs).
            easy::setopt_ptr(
                curl,
                easy::CURLOPT_ERRORBUFFER,
                entry.error_buf.as_mut_ptr() as *mut c_void,
            );
        }
        handles::lock_recover(handles::handles()).insert(id, entry);
        id
    })
}

/// Sets `CURLOPT_URL` on handle `id` from a raw byte string (not required to
/// be UTF-8: a URL is just a NUL-free byte string as far as libcurl cares).
/// Returns `0` for an unknown id, embedded NUL bytes, or a libcurl setopt
/// failure.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_set_url(id: i64, ptr: *const u8, len: usize) -> i32 {
    handles::ffi_guard(0, || {
        let Some(url) = bytes_to_cstring(ptr, len) else {
            return 0;
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        let code = unsafe { easy::setopt_str(entry.curl, easy::CURLOPT_URL, url.as_ptr()) };
        (code == easy::CURLE_OK) as i32
    })
}

/// Sets a `long`-valued `curl_setopt` option (`CURLOPT_RETURNTRANSFER` at
/// minimum — see `crate::php_layer::apply_long_option` for the full Task 3
/// option table). Returns `0` for an unknown id or a libcurl setopt failure.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_setopt_long(id: i64, opt: i32, value: i64) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        php_layer::apply_long_option(entry, opt, value) as i32
    })
}

/// Sets a string-valued `curl_setopt` option, forwarded to real
/// `curl_easy_setopt` unchanged (no PHP-only string options exist yet in
/// Task 3's scope). Returns `0` for an unknown id, embedded NUL bytes, or a
/// libcurl setopt failure.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_setopt_str(
    id: i64,
    opt: i32,
    ptr: *const u8,
    len: usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let Some(value) = bytes_to_cstring(ptr, len) else {
            return 0;
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        let code = unsafe { easy::setopt_str(entry.curl, opt as c_int, value.as_ptr()) };
        (code == easy::CURLE_OK) as i32
    })
}

/// Runs the configured transfer to completion. Resets the RETURNTRANSFER
/// capture buffer and error buffer first, so each call starts a fresh
/// capture independent of whether a previous body was taken. Returns `0` for
/// an unknown id or any non-`CURLE_OK` result; call
/// `elephc_curl_easy_errno`/`elephc_curl_easy_error` for the specific reason.
///
/// Deliberately drops the table lock before calling `curl_easy_perform`
/// (below) — the write callback needs to re-lock the table per chunk from
/// this same thread, which would deadlock otherwise. Consequence: while a
/// `perform(id)` is blocked inside libcurl, the table lock does NOT protect
/// `curl`'s underlying `*mut CURL` from a concurrent `elephc_curl_easy_free`
/// on the same `id` from another thread (see `crate::handles::EasyEntry`'s
/// `Send` impl). Sound only under this crate's caller contract: no two
/// `elephc_curl_*` calls for the same id run concurrently.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_perform(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let curl = {
            let mut guard = handles::lock_recover(handles::handles());
            let Some(entry) = guard.get_mut(&id) else {
                return 0;
            };
            entry.body.clear();
            // Zero the error buffer first: libcurl is not guaranteed to
            // write it on success (only documented to write it on error).
            entry.error_buf.iter_mut().for_each(|byte| *byte = 0);
            entry.curl
        };
        // The table lock is NOT held across the blocking transfer: the write
        // callback (crate::php_layer::write_callback) re-locks the table
        // per chunk from the same thread, which would deadlock on a
        // non-reentrant `Mutex` otherwise. See this function's doc comment
        // for the resulting caller contract (no concurrent calls on `id`).
        let code = unsafe { easy::perform(curl) };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        entry.last_errno = code;
        entry.last_error = extract_error_message(&entry.error_buf);
        (code == easy::CURLE_OK) as i32
    })
}

/// Returns the `CURLcode` from the most recent `elephc_curl_easy_perform`
/// call on `id` (`0` = `CURLE_OK`), or `0` for an unknown id or a handle that
/// has never performed a transfer — matching PHP's `curl_errno()`.
///
/// Unlike every other status return in this ABI, this is the raw `CURLcode`,
/// not a boolean: it exists specifically to back `curl_errno()`.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_errno(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        guard.get(&id).map_or(0, |entry| entry.last_errno)
    })
}

/// Copies the most recent transfer's error message into `out`. Always
/// reports the message's byte length through `out_len`, even when `out_cap`
/// is too small (`0` return) or `id` is unknown (`out_len` set to `0`).
///
/// # Safety
/// `out` must be valid for `out_cap` bytes when non-null. `out_len` must be
/// valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_error(
    id: i64,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            write_out_len(out_len, 0);
            return 0;
        };
        unsafe { publish_bytes(&entry.last_error, out, out_cap, out_len) }
    })
}

/// Reads a `long`-typed `curl_easy_getinfo` field on handle `id` into `out`
/// (e.g. `CURLINFO_RESPONSE_CODE`, PHP's `CURLINFO_HTTP_CODE` = 2097154).
/// Returns `0` — leaving `out` untouched — for an unknown id, an `info` value
/// outside libcurl's `CURLINFO_LONG` type range (see `crate::easy::
/// CURLINFO_LONG`'s doc comment for why that check exists: `curl_easy_getinfo`
/// dispatches its output type purely from `info`'s numeric bits, so this
/// wrapper never asks it to write a `long` through a pointer typed for a
/// different shape), or a libcurl `getinfo` failure.
///
/// # Safety
/// `out` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_getinfo_long(id: i64, info: i32, out: *mut i64) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            return 0;
        };
        match unsafe { easy::getinfo_long(entry.curl, info as c_int) } {
            Some(value) => {
                if !out.is_null() {
                    unsafe {
                        *out = value;
                    }
                }
                1
            }
            None => 0,
        }
    })
}

/// Transfers ownership of the RETURNTRANSFER-captured body to the caller:
/// writes a pointer/length pair through `ptr`/`len`. The returned pointer
/// stays valid until the next `elephc-curl` call that touches this `id`
/// (`elephc_curl_easy_take_body` again, `elephc_curl_easy_perform`, or
/// `elephc_curl_easy_free`) — the same "borrowed until overwritten"
/// convention `elephc-pdo`'s `store_bytes`/`elephc-image`'s `out_cell` use;
/// the caller is expected to copy the bytes out immediately. Returns `0` for
/// an unknown id.
///
/// # Safety
/// `ptr` and `len` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_take_body(
    id: i64,
    ptr: *mut *mut u8,
    len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            write_out_len(len, 0);
            return 0;
        };
        entry.taken_body = std::mem::take(&mut entry.body);
        write_out_len(len, entry.taken_body.len());
        if !ptr.is_null() {
            unsafe {
                *ptr = entry.taken_body.as_mut_ptr();
            }
        }
        1
    })
}

/// Removes handle `id` from the table and runs `curl_easy_cleanup` on it.
/// A no-op for an unknown/already-freed id (ids are never reused, so a
/// double-free is harmless here, unlike a raw libcurl double-cleanup).
///
/// Deliberately drops the table lock before calling `curl_easy_cleanup`
/// (below), matching `elephc_curl_easy_perform`'s reason for the same
/// pattern. Consequence: this removes `id` from the table (safe under the
/// mutex — nothing else can look `id` up again after this returns), but if
/// another thread is mid-`elephc_curl_easy_perform(id)` when this runs, that
/// thread's already-copied `*mut CURL` is freed out from under it — a real
/// use-after-free at the libcurl level. Sound only under this crate's caller
/// contract: no two `elephc_curl_*` calls for the same id run concurrently
/// (see `crate::handles::EasyEntry`'s `Send` impl for the full reasoning).
#[no_mangle]
pub extern "C" fn elephc_curl_easy_free(id: i64) {
    handles::ffi_guard((), || {
        let mut guard = handles::lock_recover(handles::handles());
        if let Some(entry) = guard.remove(&id) {
            // Do not hold the table lock across `curl_easy_cleanup`.
            drop(guard);
            unsafe { easy::cleanup(entry.curl) };
        }
    })
}

/// Writes the `curl_version()` JSON blob (libcurl's own
/// `curl_version_info(CURLVERSION_NOW)`, not tied to any easy handle) into
/// `out_json`. Always reports the required byte length through `len`, even
/// when `cap` is too small (`0` return).
///
/// # Safety
/// `out_json` must be valid for `cap` bytes when non-null. `len` must be
/// valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_global_info(
    out_json: *mut u8,
    cap: usize,
    len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let json = build_global_info_json();
        unsafe { publish_bytes(json.as_bytes(), out_json, cap, len) }
    })
}

/// Builds a NUL-free `CString` from a caller-supplied byte pointer/length, or
/// `None` for a null pointer with nonzero length, or embedded NUL bytes.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
unsafe fn bytes_to_cstring(ptr: *const u8, len: usize) -> Option<CString> {
    if len == 0 {
        return CString::new(Vec::new()).ok();
    }
    if ptr.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    CString::new(bytes).ok()
}

/// Extracts the NUL-terminated message libcurl wrote into a
/// `CURLOPT_ERRORBUFFER` buffer, up to (and excluding) the first NUL byte.
fn extract_error_message(buf: &[u8]) -> Vec<u8> {
    let end = buf.iter().position(|&byte| byte == 0).unwrap_or(buf.len());
    buf[..end].to_vec()
}

/// Writes `value` through `ptr` when non-null. Null-safe counterpart to the
/// `out_len`/`len` out-parameters several entry points above take.
unsafe fn write_out_len(ptr: *mut usize, value: usize) {
    if !ptr.is_null() {
        unsafe {
            *ptr = value;
        }
    }
}

/// Copies `bytes` into a caller-owned buffer, reporting the required length
/// through `out_len` unconditionally (even on failure, so the caller can
/// retry with a larger buffer) — mirrors `elephc-crypto`'s
/// `cipher::abi::publish_decrypt`. Returns `1` on success, `0` when `out_cap`
/// is too small or `out_ptr` is null for a nonempty result.
///
/// # Safety
/// `out_ptr` must be valid for `out_cap` bytes when non-null. `out_len` must
/// be valid for a write when non-null.
unsafe fn publish_bytes(bytes: &[u8], out_ptr: *mut u8, out_cap: usize, out_len: *mut usize) -> i32 {
    unsafe {
        write_out_len(out_len, bytes.len());
    }
    if bytes.len() > out_cap || (!bytes.is_empty() && out_ptr.is_null()) {
        return 0;
    }
    if !bytes.is_empty() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, bytes.len());
        }
    }
    1
}

/// Builds the `curl_version()` JSON blob from libcurl's own
/// `curl_version_info(CURLVERSION_NOW)`, matching the key set
/// `.superpowers/sdd/php-curl-family/global-constraints.md`'s "`curl_version()`
/// keys" section documents. The always-present fields
/// (`version_number`/`age`/`features`/`ssl_version_number`/`version`/`host`/
/// `ssl_version`/`libz_version`/`protocols`) match every libcurl build; the
/// optional sub-library fields (`ares`/`ares_num`, `libidn`,
/// `iconv_ver_num`/`libssh_version`, `brotli_ver_num`/`brotli_version`,
/// `feature_list`) are included only when libcurl reports a non-null
/// pointer, mirroring PHP's own `_php_curl_version` (`ext/curl/interface.c`),
/// which omits keys for libraries this libcurl build was not compiled with.
fn build_global_info_json() -> String {
    let info = easy::version_info();
    let mut map = serde_json::Map::new();
    map.insert("version_number".to_string(), info.version_num.into());
    map.insert("age".to_string(), info.age.into());
    map.insert("features".to_string(), info.features.into());
    map.insert(
        "ssl_version_number".to_string(),
        info.ssl_version_num.into(),
    );
    map.insert("version".to_string(), c_str_or_empty(info.version).into());
    map.insert("host".to_string(), c_str_or_empty(info.host).into());
    insert_optional_str(&mut map, "ssl_version", info.ssl_version);
    insert_optional_str(&mut map, "libz_version", info.libz_version);
    map.insert(
        "protocols".to_string(),
        c_str_array(info.protocols).into(),
    );
    insert_optional_str(&mut map, "ares", info.ares);
    if !info.ares.is_null() {
        map.insert("ares_num".to_string(), info.ares_num.into());
    }
    insert_optional_str(&mut map, "libidn", info.libidn);
    insert_optional_str(&mut map, "libssh_version", info.libssh_version);
    if !info.libssh_version.is_null() {
        map.insert("iconv_ver_num".to_string(), info.iconv_ver_num.into());
    }
    insert_optional_str(&mut map, "brotli_version", info.brotli_version);
    if !info.brotli_version.is_null() {
        map.insert("brotli_ver_num".to_string(), info.brotli_ver_num.into());
    }
    if !info.feature_names.is_null() {
        map.insert(
            "feature_list".to_string(),
            c_str_array(info.feature_names).into(),
        );
    }
    serde_json::Value::Object(map).to_string()
}

/// Converts a possibly-null `*const c_char` to an owned `String` (empty for
/// null), replacing invalid UTF-8 with the standard replacement character so
/// the JSON encoder stays infallible regardless of what libcurl reports.
fn c_str_or_empty(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Inserts `key => value` only when `ptr` is non-null, matching PHP's own
/// omission of keys for sub-libraries this libcurl build was not compiled
/// with.
fn insert_optional_str(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    ptr: *const c_char,
) {
    if !ptr.is_null() {
        map.insert(key.to_string(), c_str_or_empty(ptr).into());
    }
}

/// Reads a NULL-terminated `const char * const *` array of C strings (the
/// shape `curl_version_info_data`'s `protocols`/`feature_names` both use)
/// into an owned `Vec<String>`. Returns an empty vec for a null array.
fn c_str_array(ptr: *const *const c_char) -> Vec<String> {
    if ptr.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = ptr;
    unsafe {
        while !(*cursor).is_null() {
            out.push(c_str_or_empty(*cursor));
            cursor = cursor.add(1);
        }
    }
    out
}
