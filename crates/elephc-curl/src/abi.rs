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
use crate::info;
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
/// `curl_easy_setopt` unchanged — except `CURLOPT_POSTFIELDS`, which goes
/// through [`set_postfields`] because a request body is not a C string.
/// Returns `0` for an unknown id, embedded NUL bytes, or a libcurl setopt
/// failure.
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
        // The body must be read as BYTES, before any `CString` conversion: a
        // POST body may legitimately contain NUL (a PHP string is binary-safe,
        // and so is `application/octet-stream`), and `bytes_to_cstring` would
        // reject exactly that.
        if opt == easy::CURLOPT_POSTFIELDS {
            let body: &[u8] = if len == 0 {
                &[]
            } else if ptr.is_null() {
                return 0;
            } else {
                unsafe { std::slice::from_raw_parts(ptr, len) }
            };
            let mut guard = handles::lock_recover(handles::handles());
            let Some(entry) = guard.get_mut(&id) else {
                return 0;
            };
            return unsafe { set_postfields(entry, body) } as i32;
        }
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

/// Sets a `struct curl_slist *` option (`CURLOPT_HTTPHEADER`, `CURLOPT_QUOTE`,
/// `CURLOPT_RESOLVE`, ...) from a blob of NUL-TERMINATED items: each item's
/// bytes followed by one `0` byte, so an empty blob is an empty list and a
/// single empty item is one `0` byte. That framing is unambiguous where a
/// separator-joined encoding is not, and it is exactly what
/// `curl_slist_append` wants anyway.
///
/// The new list is stored on the handle (`EasyEntry::slists`) because libcurl
/// does NOT copy it: it walks the list during the transfer, so it must outlive
/// every `curl_easy_perform`. The PREVIOUS list for this option is freed only
/// after libcurl has accepted the replacement, so a failed `setopt` leaves the
/// handle exactly as it was rather than pointing at freed memory.
///
/// Returns `0` for an unknown id, a null pointer with a nonzero length, an item
/// libcurl could not allocate, or a libcurl setopt failure.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_setopt_slist(
    id: i64,
    opt: i32,
    ptr: *const u8,
    len: usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let blob: &[u8] = if len == 0 {
            &[]
        } else if ptr.is_null() {
            return 0;
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };

        let items = split_nul_terminated(blob);
        let Some(list) = (unsafe { build_slist(&items) }) else {
            return 0;
        };

        let code = unsafe { easy::setopt_slist(entry.curl, opt as c_int, list) };
        if code != easy::CURLE_OK {
            unsafe { easy::slist_free_all(list) };
            return 0;
        }
        // libcurl now points at `list`; only here is the old list unreachable.
        if let Some(previous) = entry.slists.insert(opt, list) {
            unsafe { easy::slist_free_all(previous) };
        }
        1
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

/// Reads a `double`-typed `curl_easy_getinfo` field on handle `id` into `out`
/// (`CURLINFO_TOTAL_TIME`, `CURLINFO_SPEED_DOWNLOAD`, ...). Returns `0` —
/// leaving `out` untouched — for an unknown id, an `info` outside libcurl's
/// `CURLINFO_DOUBLE` type range, or a libcurl `getinfo` failure. The type-range
/// check is the same read-side guard `elephc_curl_easy_getinfo_long` documents.
///
/// # Safety
/// `out` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_getinfo_double(
    id: i64,
    info: i32,
    out: *mut f64,
) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            return 0;
        };
        match unsafe { easy::getinfo_double(entry.curl, info as c_int) } {
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

/// `elephc_curl_easy_str_op`'s `op`: URL-encode the string argument
/// (`curl_easy_escape`).
pub const ELEPHC_CURL_STR_OP_ESCAPE: i32 = 1;
/// `elephc_curl_easy_str_op`'s `op`: URL-decode the string argument
/// (`curl_easy_unescape`).
pub const ELEPHC_CURL_STR_OP_UNESCAPE: i32 = 2;
/// `elephc_curl_easy_str_op`'s `op`: read a `CURLINFO_STRING` field
/// (`number` = the `CURLINFO_*` value).
pub const ELEPHC_CURL_STR_OP_INFO_STRING: i32 = 3;
/// `elephc_curl_easy_str_op`'s `op`: read a `CURLINFO_SLIST` field into a
/// NUL-framed item blob (`number` = the `CURLINFO_*` value).
pub const ELEPHC_CURL_STR_OP_INFO_SLIST: i32 = 4;
/// `elephc_curl_easy_str_op`'s `op`: build the whole no-`$option`
/// `curl_getinfo()` associative array as a JSON blob.
pub const ELEPHC_CURL_STR_OP_INFO_ALL: i32 = 5;
/// `elephc_curl_easy_str_op`'s `op`: build `CURLINFO_CERTINFO` as a JSON blob.
pub const ELEPHC_CURL_STR_OP_INFO_CERTINFO: i32 = 6;

/// Runs one STRING-PRODUCING operation on handle `id` and parks its result in
/// the handle's scratch buffer, for [`elephc_curl_easy_take_scratch`] to hand
/// back. Returns `0` for an unknown id, an unknown `op`, or an operation
/// libcurl could not answer (a wrong info type, a field this transfer has no
/// value for).
///
/// WHY ONE ENTRY POINT AND NOT SIX. Every string-shaped `curl_getinfo()` answer
/// — and, from Wave D, `curl_escape`/`curl_unescape` — needs the identical
/// two-step dance: produce bytes the bridge owns, then copy them into a PHP
/// string before the next call can overwrite them. Each separate entry point
/// would need its own hand-written `__rt_curl_*` helper on three targets to do
/// exactly that. Folding them into one `op` keeps that assembly written once,
/// and the `op` codes above are the whole of the added indirection.
///
/// `ptr`/`len` carry the operation's STRING argument (unused, and expected
/// empty, by the info ops above); `number` carries its INTEGER argument (the
/// `CURLINFO_*` value for the info ops).
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_str_op(
    id: i64,
    op: i32,
    ptr: *const u8,
    len: usize,
    number: i64,
) -> i32 {
    handles::ffi_guard(0, || {
        let argument: &[u8] = if len == 0 {
            &[]
        } else if ptr.is_null() {
            return 0;
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        let Ok(number) = i32::try_from(number) else {
            return 0;
        };
        let produced = match op {
            ELEPHC_CURL_STR_OP_ESCAPE => unsafe { easy::escape(entry.curl, argument) },
            ELEPHC_CURL_STR_OP_UNESCAPE => unsafe { easy::unescape(entry.curl, argument) },
            // `.flatten()`: a NULL `char *` (libcurl succeeded, this transfer has no
            // value for the field) collapses into the failure arm on purpose, so the
            // typed `curl_getinfo($ch, CURLINFO_*)` form answers `false` the way php-src
            // does rather than a fabricated empty string.
            ELEPHC_CURL_STR_OP_INFO_STRING => {
                unsafe { easy::getinfo_str(entry.curl, number) }.flatten()
            }
            ELEPHC_CURL_STR_OP_INFO_SLIST => unsafe { easy::getinfo_slist(entry.curl, number) }
                .map(|items| info::frame_items(&items)),
            ELEPHC_CURL_STR_OP_INFO_ALL => Some(unsafe { info::getinfo_all_json(entry.curl) }),
            ELEPHC_CURL_STR_OP_INFO_CERTINFO => Some(unsafe { info::certinfo_json(entry.curl) }),
            _ => None,
        };
        match produced {
            Some(bytes) => {
                entry.scratch = bytes;
                1
            }
            None => 0,
        }
    })
}

/// Hands back the bytes the last [`elephc_curl_easy_str_op`] on this `id`
/// produced, as a pointer/length pair valid until the next `elephc-curl` call
/// that touches this `id` — the same "borrowed until overwritten" convention
/// [`elephc_curl_easy_take_body`] uses, and the reason the caller copies
/// immediately. Returns `0` for an unknown id.
///
/// # Safety
/// `ptr` and `len` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_take_scratch(
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
        write_out_len(len, entry.scratch.len());
        if !ptr.is_null() {
            unsafe {
                *ptr = entry.scratch.as_mut_ptr();
            }
        }
        1
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

/// Resets every libcurl option on handle `id` to its default and clears the
/// PHP-layer state that goes with them, matching PHP's `curl_reset()`. Returns
/// `0` for an unknown id.
///
/// THREE THINGS MUST BE PUT BACK AFTERWARDS, because `curl_easy_reset` resets
/// OPTIONS and elephc's own plumbing is made of options: the write callback and
/// its `userdata` (without them a later `curl_exec()` would write the body to
/// libcurl's default stdout and the RETURNTRANSFER capture would silently stop
/// working), and `CURLOPT_ERRORBUFFER` (without it `curl_error()` would report
/// nothing forever after). The handle's slists are freed here too — libcurl
/// dropped its pointers to them in the reset, so this is the first moment they
/// are unreachable.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_reset(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        unsafe { easy::reset(entry.curl) };
        entry.free_slists();
        entry.return_transfer = false;
        entry.body.clear();
        entry.taken_body.clear();
        entry.scratch.clear();
        entry.last_errno = easy::CURLE_OK;
        entry.last_error.clear();
        entry.error_buf.iter_mut().for_each(|byte| *byte = 0);
        unsafe {
            php_layer::install_write_callback(entry.curl, id);
            easy::setopt_ptr(
                entry.curl,
                easy::CURLOPT_ERRORBUFFER,
                entry.error_buf.as_mut_ptr() as *mut c_void,
            );
        }
        1
    })
}

/// Applies a `CURLPAUSE_*` bitmask to handle `id`, returning libcurl's raw
/// `CURLcode` — which is what PHP's `curl_pause()` returns. An unknown id
/// answers `CURLE_BAD_FUNCTION_ARGUMENT` (43) rather than `CURLE_OK`, so a
/// caller cannot read "nothing happened" as success.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_pause(id: i64, bitmask: i32) -> i32 {
    handles::ffi_guard(CURLE_BAD_FUNCTION_ARGUMENT, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            return CURLE_BAD_FUNCTION_ARGUMENT;
        };
        unsafe { easy::pause(entry.curl, bitmask as c_int) }
    })
}

/// `CURLE_BAD_FUNCTION_ARGUMENT` (43): the `CURLcode`
/// `elephc_curl_easy_pause` reports for an unknown handle id.
const CURLE_BAD_FUNCTION_ARGUMENT: i32 = 43;

/// Runs `curl_easy_upkeep` on handle `id`. Returns `1` only when libcurl
/// reported `CURLE_OK`, matching PHP's `curl_upkeep(): bool`.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_upkeep(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            return 0;
        };
        (unsafe { easy::upkeep(entry.curl) } == easy::CURLE_OK) as i32
    })
}

/// Duplicates handle `id` — libcurl options AND the bridge's own PHP-layer
/// state — and registers the copy under a fresh id, which it returns. `0` for
/// an unknown id or a libcurl allocation failure.
///
/// THE COPY'S CALLBACK PLUMBING IS REINSTALLED, not inherited.
/// `curl_easy_duphandle` copies every option VALUE, which means the copy would
/// otherwise point `CURLOPT_WRITEDATA` at the ORIGINAL handle's id (so the
/// copy's response body would land in the original's capture buffer) and
/// `CURLOPT_ERRORBUFFER` at the original entry's buffer (a dangling write the
/// moment the original is freed). Both are re-pointed at the new entry here.
///
/// EVERY LIST OPTION IS REBUILT FROM SCRATCH FOR THE COPY, and that is a
/// USE-AFTER-FREE FIX, not tidiness. `curl_easy_duphandle` does NOT duplicate
/// `struct curl_slist *` options: `dupset` (libcurl 8.21.0, `lib/easy.c`) starts
/// with a shallow `dst->set = src->set` and then re-duplicates only the strings,
/// the blobs, `CURLOPT_COPYPOSTFIELDS` and the mime part — every slist pointer
/// rides the struct copy verbatim, because libcurl treats slists as
/// APPLICATION-owned. So a copy that inherited the pointer would be reading a
/// list this bridge owns and frees on the SOURCE's behalf:
///
///     $a = curl_init(...); curl_setopt($a, CURLOPT_HTTPHEADER, [...]);
///     $b = curl_copy_handle($a);
///     unset($a);            // EasyEntry::free_slists frees the list
///     curl_exec($b);        // libcurl walks freed memory
///
/// `curl_reset($a)` and simply setting `CURLOPT_HTTPHEADER` on `$a` again reach
/// the same dangling read. Copying the bridge's own map would be worse still —
/// two entries owning one list, hence a double free. Rebuilding is the only
/// shape where each handle owns exactly what it points at.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_duphandle(id: i64) -> i64 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());

        // Snapshot everything the copy needs while the source is borrowed, list
        // options included: `easy::read_slist` walks each list into owned bytes
        // so the rebuild below never reads the source's memory again.
        let (copied, return_transfer, body, last_errno, last_error, slist_items) = {
            let Some(source) = guard.get(&id) else {
                return 0;
            };
            let slist_items: Vec<(i32, Vec<Vec<u8>>)> = source
                .slists
                .iter()
                .map(|(&opt, &list)| (opt, unsafe { easy::read_slist(list) }))
                .collect();
            let copied = unsafe { easy::duphandle(source.curl) };
            if copied.is_null() {
                return 0;
            }
            (
                copied,
                source.return_transfer,
                source.body.clone(),
                source.last_errno,
                source.last_error.clone(),
                slist_items,
            )
        };

        let new_id = handles::next_id();
        let mut entry = EasyEntry::new(copied);
        entry.return_transfer = return_transfer;
        entry.body = body;
        entry.last_errno = last_errno;
        entry.last_error = last_error;
        unsafe {
            php_layer::install_write_callback(copied, new_id);
            easy::setopt_ptr(
                copied,
                easy::CURLOPT_ERRORBUFFER,
                entry.error_buf.as_mut_ptr() as *mut c_void,
            );
        }

        for (opt, items) in slist_items {
            // A rebuild this build cannot complete must CLEAR the option on the
            // copy, never leave it pointing at the source's list: the whole point
            // of this loop is that the inherited pointer is not safe to keep.
            let rebuilt = unsafe { build_slist(&items) }.unwrap_or(std::ptr::null_mut());
            let code = unsafe { easy::setopt_slist(copied, opt as c_int, rebuilt) };
            if code == easy::CURLE_OK {
                entry.slists.insert(opt, rebuilt);
            } else {
                unsafe {
                    easy::slist_free_all(rebuilt);
                    easy::setopt_slist(copied, opt as c_int, std::ptr::null_mut());
                }
            }
        }

        guard.insert(new_id, entry);
        new_id
    })
}

/// Copies libcurl's own message for a `CURLcode` into `out`, for PHP's
/// `curl_strerror()`. Handle-free: a `CURLcode`'s text does not depend on any
/// transfer. Always reports the message length through `out_len`, even when
/// `out_cap` is too small (`0` return).
///
/// # Safety
/// `out` must be valid for `out_cap` bytes when non-null. `out_len` must be
/// valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_strerror(
    code: i32,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let message = easy::strerror(code as c_int);
        unsafe { publish_bytes(&message, out, out_cap, out_len) }
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
        if let Some(mut entry) = guard.remove(&id) {
            // Do not hold the table lock across `curl_easy_cleanup`.
            drop(guard);
            unsafe { easy::cleanup(entry.curl) };
            // AFTER cleanup, never before: libcurl holds raw pointers to the
            // handle's slist options until the handle itself is gone.
            entry.free_slists();
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

/// Splits a blob of NUL-TERMINATED items into its items, dropping a trailing
/// empty tail. `b""` is zero items; `b"\0"` is one empty item; `b"a\0b\0"` is
/// two. See [`elephc_curl_easy_setopt_slist`] for why this framing rather than
/// a separator.
fn split_nul_terminated(blob: &[u8]) -> Vec<&[u8]> {
    let mut items = Vec::new();
    let mut rest = blob;
    while let Some(end) = rest.iter().position(|&byte| byte == 0) {
        items.push(&rest[..end]);
        rest = &rest[end + 1..];
    }
    // Anything after the last NUL is an unterminated fragment the caller did
    // not frame; the prelude always terminates, so this only guards a caller
    // that does not.
    if !rest.is_empty() {
        items.push(rest);
    }
    items
}

/// Builds a fresh `struct curl_slist` from owned byte items, or `None` when an item
/// carries an embedded NUL (which would silently truncate it) or libcurl could not
/// allocate — freeing whatever had been built in that case, so a failure leaks nothing
/// and hands back no half-list.
///
/// An EMPTY item list yields a null list, which is not a failure: `curl_easy_setopt` with
/// a null `struct curl_slist *` is how an option is CLEARED, and that is exactly what
/// `curl_setopt($ch, CURLOPT_HTTPHEADER, [])` means.
///
/// # Safety
/// The returned list, when non-null, is the caller's to free with
/// `easy::slist_free_all` once no live easy handle still points at it.
unsafe fn build_slist(items: &[impl AsRef<[u8]>]) -> Option<*mut easy::CurlSlist> {
    let mut list: *mut easy::CurlSlist = std::ptr::null_mut();
    for item in items {
        let Ok(item) = CString::new(item.as_ref()) else {
            unsafe { easy::slist_free_all(list) };
            return None;
        };
        match unsafe { easy::slist_append(list, item.as_ptr()) } {
            Some(next) => list = next,
            None => {
                unsafe { easy::slist_free_all(list) };
                return None;
            }
        }
    }
    Some(list)
}

/// Applies `CURLOPT_POSTFIELDS` as libcurl's copying, length-aware pair:
/// `CURLOPT_POSTFIELDSIZE_LARGE` first (so libcurl knows how many bytes to
/// take instead of calling `strlen` and truncating at the first NUL), then
/// `CURLOPT_COPYPOSTFIELDS` (so libcurl owns the bytes and the PHP string that
/// supplied them can die immediately).
///
/// php-src does exactly this in `_php_curl_setopt`, and for the same two
/// reasons: `CURLOPT_POSTFIELDS` alone would leave libcurl holding a borrowed
/// pointer into a PHP string.
///
/// # Safety
/// `entry.curl` must be a still-live handle. `body` is only read for the
/// duration of the call; libcurl copies it.
unsafe fn set_postfields(entry: &mut EasyEntry, body: &[u8]) -> bool {
    let size = unsafe {
        easy::setopt_off_t(
            entry.curl,
            easy::CURLOPT_POSTFIELDSIZE_LARGE,
            body.len() as i64,
        )
    };
    if size != easy::CURLE_OK {
        return false;
    }
    // A zero-length body still needs a non-null pointer: libcurl treats null
    // as "unset the option" rather than "post nothing".
    let ptr = if body.is_empty() {
        c"".as_ptr()
    } else {
        body.as_ptr() as *const c_char
    };
    let code = unsafe { easy::setopt_str(entry.curl, easy::CURLOPT_COPYPOSTFIELDS, ptr) };
    code == easy::CURLE_OK
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
