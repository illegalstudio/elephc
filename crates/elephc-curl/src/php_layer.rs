//! Purpose:
//! PHP-facing easy-handle semantics that libcurl itself has no concept of:
//! the `CURLOPT_RETURNTRANSFER` pseudo-option and the write callback that
//! implements it (capture into `EasyEntry::body`, or stream to stdout by
//! default so `curl_exec()` output matches PHP CLI).
//!
//! Called from:
//! - `crate::abi::elephc_curl_easy_init` installs [`write_callback`] on every
//!   handle unconditionally.
//! - `crate::abi::elephc_curl_easy_setopt_long` routes through
//!   [`apply_long_option`] for `CURLOPT_RETURNTRANSFER` handling.
//!
//! Key details:
//! - Only `CURLOPT_RETURNTRANSFER` is implemented here (Task 3's scope).
//!   `CURLOPT_HEADER`, `CURLOPT_FILE`, `CURLOPT_INFILE`, `CURLOPT_WRITEHEADER`,
//!   `CURLOPT_STDERR`, and `CURLOPT_BINARYTRANSFER`/`CURLOPT_SAFE_UPLOAD` are
//!   the rest of the PHP-layer option table
//!   (`.superpowers/sdd/php-curl-family/global-constraints.md`); Task 8 adds
//!   them. Every other `long` option is forwarded to real
//!   `curl_easy_setopt` unchanged.
//! - `write_callback` must never unwind across the C boundary: libcurl calls
//!   it directly, not through one of this crate's own `#[no_mangle]` entry
//!   points, so it carries its own panic firewall rather than relying on
//!   `crate::handles::ffi_guard` (which assumes an `extern "C"` entry, not a
//!   function-pointer callback).

use std::ffi::{c_char, c_int, c_long, c_void};

use crate::callbacks;
use crate::easy::{self, CURL};
use crate::handles::{self, EasyEntry};

/// `CURLOPT_RETURNTRANSFER` (19913): a PHP-only pseudo-option, never a real
/// libcurl `CURLOPT_*` value. Frozen from `scripts/docs/curl_surface.json`
/// (Task 1's PHP surface extraction), not recomputed by hand. PHP's own
/// `php_only_options` list confirms this is never forwarded to libcurl.
pub(crate) const CURLOPT_RETURNTRANSFER: i32 = 19913;

/// The first `CURLOPTTYPE_OFF_T` option number. libcurl reads the variadic
/// argument of every option at or above this (and below the blob base) as a
/// `curl_off_t` rather than a `long` (libcurl 8.21.0, `lib/setopt.c`).
pub(crate) const CURLOPTTYPE_OFF_T: i32 = 30_000;

/// One past the last `CURLOPTTYPE_OFF_T` option number (`CURLOPTTYPE_BLOB`).
pub(crate) const CURLOPTTYPE_BLOB: i32 = 40_000;

/// Applies an integer-valued `curl_setopt`. `CURLOPT_RETURNTRANSFER` flips
/// whether the write callback captures the body into `entry.body` instead of
/// streaming it to stdout; it is handled entirely on the Rust side and never
/// reaches libcurl. Every other option forwards to real `curl_easy_setopt`
/// unchanged, returning whether libcurl accepted it.
///
/// An option in libcurl's `CURLOPTTYPE_OFF_T` range goes through
/// `easy::setopt_off_t` rather than `easy::setopt_long`, so the variadic
/// argument matches the type libcurl reads. The caller (the curl prelude,
/// through `elephc_curl_option_kind`) has already refused every option this
/// build cannot carry, so nothing outside the `long`/`off_t` ranges reaches
/// here in a compiled program; the range check is what keeps that true even if
/// a future caller forgets.
pub(crate) fn apply_long_option(entry: &mut EasyEntry, opt: i32, value: i64) -> bool {
    if opt == CURLOPT_RETURNTRANSFER {
        entry.return_transfer = value != 0;
        // php-src keeps ONE write mode, not a flag per sink: `CURLOPT_RETURNTRANSFER`
        // assigns `handlers.write->method = PHP_CURL_RETURN`, which deselects a
        // previously installed `CURLOPT_WRITEFUNCTION` (`PHP_CURL_USER`). Measured on
        // PHP 8.4.20: setting WRITEFUNCTION and then RETURNTRANSFER makes `curl_exec()`
        // return the body and never calls the callback. The callable itself stays
        // rooted (php-src keeps `func_name` too), so re-setting WRITEFUNCTION later
        // reinstates it.
        entry.write_user = false;
        return true;
    }
    if (CURLOPTTYPE_OFF_T..CURLOPTTYPE_BLOB).contains(&opt) {
        return unsafe { easy::setopt_off_t(entry.curl, opt as c_int, value) == easy::CURLE_OK };
    }
    unsafe { easy::setopt_long(entry.curl, opt as c_int, value as c_long) == easy::CURLE_OK }
}

/// Installs [`write_callback`] and its `userdata` (the handle's own `id`) on
/// a freshly initialized handle. Called unconditionally from
/// `elephc_curl_easy_init` — the default (non-RETURNTRANSFER) behavior needs
/// the callback too, to reach stdout the same way PHP CLI does.
///
/// # Safety
/// `curl` must be a still-live pointer from `easy::init` that has not yet had
/// `CURLOPT_WRITEFUNCTION`/`CURLOPT_WRITEDATA` set by anything else.
pub(crate) unsafe fn install_write_callback(curl: *mut CURL, id: i64) {
    let _ = easy::setopt_write_function(curl, easy::CURLOPT_WRITEFUNCTION, write_callback);
    // `CURLOPT_WRITEDATA` takes an opaque `void *userdata` that this crate
    // controls both ends of: it is never dereferenced as a real pointer, only
    // cast back to the `i64` id it started as (see `write_callback` below).
    let _ = easy::setopt_ptr(curl, easy::CURLOPT_WRITEDATA, id as usize as *mut c_void);
}

/// The `curl_write_callback` every easy handle registers. `userdata` carries
/// the handle's `i64` id (see [`install_write_callback`]): this looks the
/// handle back up in the shared table to decide whether to capture the chunk
/// into `body` (`RETURNTRANSFER`) or stream it to fd 1 (PHP CLI's default).
///
/// Returning fewer bytes than were given (including `0`) tells libcurl the
/// write failed and aborts the transfer with `CURLE_WRITE_ERROR` — used here
/// both for an unknown/already-freed id and for a real fd-1 write failure.
///
/// # Safety
/// Called by libcurl with `buffer` valid for `size * nitems` bytes for the
/// duration of the call. Must not unwind across the C boundary — libcurl
/// calls this directly, so an escaping panic is undefined behavior, not just
/// a caught error like the `ffi_guard`-wrapped `#[no_mangle]` entry points.
unsafe extern "C" fn write_callback(
    buffer: *mut c_char,
    size: usize,
    nitems: usize,
    userdata: *mut c_void,
) -> usize {
    let total = size.saturating_mul(nitems);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if total == 0 {
            return 0usize;
        }
        // SAFETY: libcurl guarantees `buffer` is valid for `total` bytes for
        // the duration of this call.
        let chunk = std::slice::from_raw_parts(buffer as *const u8, total);
        let id = userdata as i64;
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0usize;
        };
        if entry.write_user {
            // Do not hold the table lock across compiled PHP: the callback may call
            // `curl_setopt()`/`curl_exec()` on other handles and re-enter this table.
            drop(guard);
            crate::callbacks::run_bytes_callback(id, callbacks::SLOT_WRITE as usize, chunk)
        } else if entry.return_transfer {
            entry.body.extend_from_slice(chunk);
            total
        } else {
            // Do not hold the table lock across a blocking syscall.
            drop(guard);
            write_all_stdout(chunk)
        }
    }));
    outcome.unwrap_or(0)
}

/// Writes every byte of `bytes` to fd 1, looping over short writes (a single
/// `libc::write` may write fewer bytes than requested, e.g. to a pipe).
/// Returns the number of bytes actually written, so a partial/failed
/// underlying write correctly reports back to libcurl as a short write.
fn write_all_stdout(bytes: &[u8]) -> usize {
    let mut written = 0usize;
    while written < bytes.len() {
        // SAFETY: `bytes[written..]` is a valid slice for its own length;
        // `libc::write` only reads from it.
        let n = unsafe {
            libc::write(
                1,
                bytes[written..].as_ptr() as *const c_void,
                bytes.len() - written,
            )
        };
        if n <= 0 {
            break;
        }
        written += n as usize;
    }
    written
}
