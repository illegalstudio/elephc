//! Purpose:
//! Raw `elephc_curl_*` C ABI declarations plus safe Rust wrappers for the eval curl
//! builtins (`crate::interpreter::builtins::curl`). This module owns EVERY unresolved
//! reference to the bridge: nothing else in this crate declares `elephc_curl_*` directly.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl`'s home files.
//! - `crate::stream_resources::types`'s `EvalStreamResources::drop`, to free any curl
//!   easy handles an eval context still owns when it is torn down.
//!
//! Key details:
//! - COMPILED ONLY BEHIND THE `curl` CARGO FEATURE (see `Cargo.toml`'s header), which is
//!   OFF by default. `crate::interpreter::builtins::curl`'s module doc has the full
//!   argument for why: these `extern "C"` declarations reference symbols this crate does
//!   not itself provide, exactly like `crates/elephc-curl`'s own declarations of raw
//!   libcurl symbols. `cargo build`/`cargo test -p elephc-magician` WITHOUT `--features
//!   curl` never compiles this file at all, so it costs nothing for every existing
//!   consumer of this crate.
//! - MIRRORS `crates/elephc-curl/src/abi.rs` EXACTLY: every signature here is copied from
//!   that file's `#[no_mangle] pub extern "C" fn` declarations, not reinvented. Consult
//!   that file (not libcurl's own docs) for what each entry point actually does.
//! - NO REIMPLEMENTED HTTP. Every wrapper below does nothing but marshal bytes across the
//!   FFI boundary and call the bridge; the transfer itself, TLS, and every `CURLOPT_*`
//!   semantic all live in `elephc-curl` and, transitively, the pinned libcurl it links.

// The `CurlHandle`/`CurlMultiHandle`/`CurlShareHandle` OPTION KIND CODES
// `elephc_curl_option_kind()` answers, copied verbatim from
// `crates/elephc-curl/src/options.rs`'s `KIND_*` constants (that module is `pub(crate)`
// inside `elephc-curl` and this crate has no path dependency on it — see this module's
// header — so the codes are forked data, exactly like `EVAL_CURL_INT_CONSTANTS`).
pub(crate) const KIND_INVALID: i32 = 0;
pub(crate) const KIND_LONG: i32 = 1;
pub(crate) const KIND_STRING: i32 = 2;
pub(crate) const KIND_SLIST: i32 = 3;
pub(crate) const KIND_OFF_T: i32 = 4;
pub(crate) const KIND_PHP_LAYER: i32 = 5;
pub(crate) const KIND_UNSUPPORTED: i32 = 6;
pub(crate) const KIND_SHARE: i32 = 7;
pub(crate) const KIND_CALLBACK: i32 = 8;

// `elephc_curl_easy_str_op`'s `op` codes, copied verbatim from
// `crates/elephc-curl/src/abi.rs`'s `ELEPHC_CURL_STR_OP_*` constants.
pub(crate) const STR_OP_ESCAPE: i32 = 1;
pub(crate) const STR_OP_UNESCAPE: i32 = 2;
pub(crate) const STR_OP_INFO_STRING: i32 = 3;
pub(crate) const STR_OP_INFO_SLIST: i32 = 4;
pub(crate) const STR_OP_INFO_ALL: i32 = 5;
pub(crate) const STR_OP_INFO_CERTINFO: i32 = 6;

unsafe extern "C" {
    fn elephc_curl_option_kind(opt: i64) -> i32;
    fn elephc_curl_easy_init() -> i64;
    fn elephc_curl_easy_set_url(id: i64, ptr: *const u8, len: usize) -> i32;
    fn elephc_curl_easy_setopt_long(id: i64, opt: i32, value: i64) -> i32;
    fn elephc_curl_easy_setopt_str(id: i64, opt: i32, ptr: *const u8, len: usize) -> i32;
    fn elephc_curl_easy_setopt_slist(id: i64, opt: i32, ptr: *const u8, len: usize) -> i32;
    fn elephc_curl_easy_perform(id: i64) -> i32;
    fn elephc_curl_easy_errno(id: i64) -> i32;
    fn elephc_curl_easy_error(id: i64, out: *mut u8, out_cap: usize, out_len: *mut usize) -> i32;
    fn elephc_curl_easy_getinfo_long(id: i64, info: i32, out: *mut i64) -> i32;
    fn elephc_curl_easy_getinfo_double(id: i64, info: i32, out: *mut f64) -> i32;
    fn elephc_curl_easy_str_op(
        id: i64,
        op: i32,
        ptr: *const u8,
        len: usize,
        number: i64,
    ) -> i32;
    fn elephc_curl_easy_take_scratch(id: i64, ptr: *mut *mut u8, len: *mut usize) -> i32;
    fn elephc_curl_easy_take_body(id: i64, ptr: *mut *mut u8, len: *mut usize) -> i32;
    fn elephc_curl_easy_reset(id: i64) -> i32;
    fn elephc_curl_easy_pause(id: i64, bitmask: i32) -> i32;
    fn elephc_curl_easy_upkeep(id: i64) -> i32;
    fn elephc_curl_easy_duphandle(id: i64) -> i64;
    fn elephc_curl_strerror(code: i32, out: *mut u8, out_cap: usize, out_len: *mut usize) -> i32;
    fn elephc_curl_easy_free(id: i64);
    fn elephc_curl_global_info(out_json: *mut u8, cap: usize, len: *mut usize) -> i32;
}

/// Copies bytes out of a "probe for length, then fill" ABI entry point (the
/// `elephc_curl_easy_error`/`elephc_curl_strerror`/`elephc_curl_global_info` shape):
/// every one of them reports the required length through `out_len` even on the `0`-cap
/// probe call, exactly like `elephc-curl`'s own `publish_bytes` documents.
fn read_sized(
    mut call: impl FnMut(*mut u8, usize, *mut usize) -> i32,
) -> Vec<u8> {
    let mut len: usize = 0;
    call(std::ptr::null_mut(), 0, &mut len);
    if len == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0_u8; len];
    let mut written = 0_usize;
    if call(buffer.as_mut_ptr(), buffer.len(), &mut written) == 0 {
        return Vec::new();
    }
    buffer.truncate(written.min(buffer.len()));
    buffer
}

/// Classifies a `curl_setopt()` option number. See `KIND_*` above.
pub(crate) fn option_kind(opt: i64) -> i32 {
    unsafe { elephc_curl_option_kind(opt) }
}

/// Allocates a fresh easy handle. `None` on libcurl allocation failure.
pub(crate) fn easy_init() -> Option<i64> {
    let id = unsafe { elephc_curl_easy_init() };
    (id != 0).then_some(id)
}

/// Sets `CURLOPT_URL` from raw bytes.
pub(crate) fn easy_set_url(id: i64, url: &[u8]) -> bool {
    unsafe { elephc_curl_easy_set_url(id, url.as_ptr(), url.len()) != 0 }
}

/// Sets a `long`-valued option.
pub(crate) fn easy_setopt_long(id: i64, opt: i32, value: i64) -> bool {
    unsafe { elephc_curl_easy_setopt_long(id, opt, value) != 0 }
}

/// Sets a string-valued option from raw (possibly NUL-containing) bytes.
pub(crate) fn easy_setopt_str(id: i64, opt: i32, value: &[u8]) -> bool {
    unsafe { elephc_curl_easy_setopt_str(id, opt, value.as_ptr(), value.len()) != 0 }
}

/// Sets a `struct curl_slist *` option from a NUL-FRAMED item blob (see
/// `elephc-curl`'s own doc: each item's bytes followed by one `0` byte).
pub(crate) fn easy_setopt_slist(id: i64, opt: i32, blob: &[u8]) -> bool {
    unsafe { elephc_curl_easy_setopt_slist(id, opt, blob.as_ptr(), blob.len()) != 0 }
}

/// Runs the configured transfer to completion.
pub(crate) fn easy_perform(id: i64) -> bool {
    unsafe { elephc_curl_easy_perform(id) != 0 }
}

/// Returns the most recent transfer's raw `CURLcode` (`0` = `CURLE_OK`).
pub(crate) fn easy_errno(id: i64) -> i32 {
    unsafe { elephc_curl_easy_errno(id) }
}

/// Returns the most recent transfer's error message (empty when there was none).
pub(crate) fn easy_error(id: i64) -> Vec<u8> {
    read_sized(|ptr, cap, len| unsafe { elephc_curl_easy_error(id, ptr, cap, len) })
}

/// Reads a `long`-typed `curl_easy_getinfo` field.
pub(crate) fn easy_getinfo_long(id: i64, info: i32) -> Option<i64> {
    let mut out: i64 = 0;
    (unsafe { elephc_curl_easy_getinfo_long(id, info, &mut out) } != 0).then_some(out)
}

/// Reads a `double`-typed `curl_easy_getinfo` field.
pub(crate) fn easy_getinfo_double(id: i64, info: i32) -> Option<f64> {
    let mut out: f64 = 0.0;
    (unsafe { elephc_curl_easy_getinfo_double(id, info, &mut out) } != 0).then_some(out)
}

/// Runs one string-producing scratch operation (`STR_OP_*`) and copies its result out
/// immediately, honouring the "borrowed until overwritten" convention
/// `elephc_curl_easy_take_scratch` documents.
pub(crate) fn easy_str_op(id: i64, op: i32, argument: &[u8], number: i64) -> Option<Vec<u8>> {
    let produced =
        unsafe { elephc_curl_easy_str_op(id, op, argument.as_ptr(), argument.len(), number) };
    if produced == 0 {
        return None;
    }
    let mut ptr: *mut u8 = std::ptr::null_mut();
    let mut len: usize = 0;
    if unsafe { elephc_curl_easy_take_scratch(id, &mut ptr, &mut len) } == 0 {
        return None;
    }
    if len == 0 || ptr.is_null() {
        return Some(Vec::new());
    }
    Some(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec())
}

/// Returns the RETURNTRANSFER-captured body, copied out immediately (the same
/// "borrowed until overwritten" convention `easy_str_op` uses).
pub(crate) fn easy_take_body(id: i64) -> Option<Vec<u8>> {
    let mut ptr: *mut u8 = std::ptr::null_mut();
    let mut len: usize = 0;
    if unsafe { elephc_curl_easy_take_body(id, &mut ptr, &mut len) } == 0 {
        return None;
    }
    if len == 0 || ptr.is_null() {
        return Some(Vec::new());
    }
    Some(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec())
}

/// Resets every libcurl option on the handle to its default.
pub(crate) fn easy_reset(id: i64) -> bool {
    unsafe { elephc_curl_easy_reset(id) != 0 }
}

/// Applies a `CURLPAUSE_*` bitmask, returning libcurl's raw `CURLcode`.
pub(crate) fn easy_pause(id: i64, bitmask: i32) -> i32 {
    unsafe { elephc_curl_easy_pause(id, bitmask) }
}

/// Runs `curl_easy_upkeep`.
pub(crate) fn easy_upkeep(id: i64) -> bool {
    unsafe { elephc_curl_easy_upkeep(id) != 0 }
}

/// Duplicates the handle. `None` on an unknown id or libcurl allocation failure.
pub(crate) fn easy_duphandle(id: i64) -> Option<i64> {
    let copy = unsafe { elephc_curl_easy_duphandle(id) };
    (copy != 0).then_some(copy)
}

/// Returns libcurl's own message for a `CURLcode` (empty for a code libcurl does not
/// recognize).
pub(crate) fn strerror(code: i32) -> Vec<u8> {
    read_sized(|ptr, cap, len| unsafe { elephc_curl_strerror(code, ptr, cap, len) })
}

/// Removes the handle from the bridge's table and runs `curl_easy_cleanup` on it.
/// A no-op for an unknown/already-freed id.
pub(crate) fn easy_free(id: i64) {
    unsafe { elephc_curl_easy_free(id) }
}

/// Returns the `curl_version()` JSON blob (empty when the bridge could not produce it).
pub(crate) fn global_info_json() -> Vec<u8> {
    read_sized(|ptr, cap, len| unsafe { elephc_curl_global_info(ptr, cap, len) })
}
