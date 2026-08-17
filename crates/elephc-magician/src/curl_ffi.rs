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
/// The four PHP-STREAM options (`CURLOPT_FILE`, `CURLOPT_INFILE`/`CURLOPT_READDATA`,
/// `CURLOPT_WRITEHEADER`, `CURLOPT_STDERR`). The AOT build implements them in the curl
/// prelude by composing its callback slots; `eval()` carries neither the slots nor a PHP
/// stream it could hand them, so they take the same honest "not supported by this build"
/// warning + `false` that `KIND_CALLBACK` and `KIND_SHARE` do.
pub(crate) const KIND_STREAM: i32 = 9;

// `elephc_curl_easy_str_op`'s `op` codes, copied verbatim from
// `crates/elephc-curl/src/abi.rs`'s `ELEPHC_CURL_STR_OP_*` constants.
pub(crate) const STR_OP_ESCAPE: i32 = 1;
pub(crate) const STR_OP_UNESCAPE: i32 = 2;
pub(crate) const STR_OP_INFO_STRING: i32 = 3;
pub(crate) const STR_OP_INFO_SLIST: i32 = 4;
pub(crate) const STR_OP_INFO_ALL: i32 = 5;
pub(crate) const STR_OP_INFO_CERTINFO: i32 = 6;

// `elephc_curl_multi_setopt`'s three-way answer, copied verbatim from
// `crates/elephc-curl/src/multi.rs`'s `MULTI_SETOPT_*` constants: `1` applied, `0` a real
// PHP option this build cannot carry (or one libcurl refused) -> `false` plus a warning,
// `-1` not a cURL multi option at all -> php-src's own `ValueError`.
pub(crate) const MULTI_SETOPT_APPLIED: i32 = 1;
pub(crate) const MULTI_SETOPT_UNSUPPORTED: i32 = 0;
pub(crate) const MULTI_SETOPT_INVALID: i32 = -1;

// `elephc_curl_multi_info_read`'s `field` selectors, copied verbatim from
// `crates/elephc-curl/src/multi.rs`'s `INFO_FIELD_*` constants. `ADVANCE` pops one message
// off libcurl's queue (destructively) and parks it; the other four read the parked
// message's fields.
pub(crate) const INFO_FIELD_ADVANCE: i64 = 0;
pub(crate) const INFO_FIELD_MSG: i64 = 1;
pub(crate) const INFO_FIELD_RESULT: i64 = 2;
pub(crate) const INFO_FIELD_EASY_ID: i64 = 3;
pub(crate) const INFO_FIELD_QUEUED: i64 = 4;

// `elephc_curl_share_setopt`'s three-way answer, copied verbatim from
// `crates/elephc-curl/src/share.rs`'s `SHARE_SETOPT_*` constants. Unlike the multi side
// there is no "real option this build cannot carry" bucket — `REFUSED` is a value libcurl
// itself declined, which is a plain `false` and never a fabricated warning.
pub(crate) const SHARE_SETOPT_APPLIED: i32 = 1;
pub(crate) const SHARE_SETOPT_REFUSED: i32 = 0;
pub(crate) const SHARE_SETOPT_INVALID: i32 = -1;

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
    fn elephc_curl_multi_init() -> i64;
    fn elephc_curl_multi_add(multi_id: i64, easy_id: i64) -> i32;
    fn elephc_curl_multi_remove(multi_id: i64, easy_id: i64) -> i32;
    fn elephc_curl_multi_perform(multi_id: i64) -> i64;
    fn elephc_curl_multi_select(multi_id: i64, timeout_ms: i64) -> i32;
    fn elephc_curl_multi_info_read(multi_id: i64, field: i64) -> i64;
    fn elephc_curl_multi_setopt(multi_id: i64, opt: i64, value: i64) -> i32;
    fn elephc_curl_multi_errno(multi_id: i64) -> i32;
    fn elephc_curl_multi_strerror(code: i32, out: *mut u8, out_cap: usize, out_len: *mut usize)
        -> i32;
    fn elephc_curl_multi_free(multi_id: i64);
    fn elephc_curl_share_init() -> i64;
    fn elephc_curl_share_persistent_init(ptr: *const u8, len: usize) -> i64;
    fn elephc_curl_share_setopt(share_id: i64, opt: i64, value: i64) -> i32;
    fn elephc_curl_share_errno(share_id: i64) -> i32;
    fn elephc_curl_share_strerror(code: i32, out: *mut u8, out_cap: usize, out_len: *mut usize)
        -> i32;
    fn elephc_curl_easy_set_share(easy_id: i64, share_id: i64) -> i32;
    fn elephc_curl_share_free(share_id: i64);
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

/// Allocates a fresh multi handle. `None` on libcurl allocation failure.
pub(crate) fn multi_init() -> Option<i64> {
    let id = unsafe { elephc_curl_multi_init() };
    (id != 0).then_some(id)
}

/// Attaches an easy handle to a multi handle, returning libcurl's raw `CURLMcode`
/// (`0` = `CURLM_OK`) — exactly `curl_multi_add_handle()`'s own return value.
pub(crate) fn multi_add(multi: i64, easy: i64) -> i64 {
    i64::from(unsafe { elephc_curl_multi_add(multi, easy) })
}

/// Detaches an easy handle from a multi handle, returning libcurl's raw `CURLMcode`.
pub(crate) fn multi_remove(multi: i64, easy: i64) -> i64 {
    i64::from(unsafe { elephc_curl_multi_remove(multi, easy) })
}

/// Drives every attached transfer as far as it can go without blocking, answering
/// `(still_running, CURLMcode)`.
///
/// The bridge packs BOTH of `curl_multi_exec()`'s outputs into one `i64` — the count in
/// the high 32 bits and the code in the low 32 — and the low half is SIGN-EXTENDED here,
/// exactly as `crate::curl_prelude::curl_multi_exec` does it in PHP: a `CURLMcode` can be
/// negative (`CURLM_CALL_MULTI_PERFORM` is `-1`) and the packing carries it as an unsigned
/// 32-bit field, so without this `-1` would surface as `4294967295`.
pub(crate) fn multi_perform(multi: i64) -> (i64, i64) {
    let packed = unsafe { elephc_curl_multi_perform(multi) };
    let running = (packed >> 32) & 0xFFFF_FFFF;
    let mut code = packed & 0xFFFF_FFFF;
    if code >= 0x8000_0000 {
        code -= 0x1_0000_0000;
    }
    (running, code)
}

/// Waits up to `timeout_ms` for an attached transfer to become ready, answering the number
/// of ready descriptors or `-1` on a libcurl error — `curl_multi_select()`'s contract.
pub(crate) fn multi_select(multi: i64, timeout_ms: i64) -> i64 {
    i64::from(unsafe { elephc_curl_multi_select(multi, timeout_ms) })
}

/// Reads the multi handle's completion queue one field at a time (see the `INFO_FIELD_*`
/// constants above).
pub(crate) fn multi_info_read(multi: i64, field: i64) -> i64 {
    unsafe { elephc_curl_multi_info_read(multi, field) }
}

/// Applies an integer-valued `CURLMOPT_*` option, answering one of the `MULTI_SETOPT_*`
/// codes above.
pub(crate) fn multi_setopt(multi: i64, opt: i64, value: i64) -> i32 {
    unsafe { elephc_curl_multi_setopt(multi, opt, value) }
}

/// Returns the `CURLMcode` from the most recent multi operation (`curl_multi_errno()`).
pub(crate) fn multi_errno(multi: i64) -> i64 {
    i64::from(unsafe { elephc_curl_multi_errno(multi) })
}

/// Returns libcurl's own message for a `CURLMcode` — a DIFFERENT numbering space from
/// `CURLcode`, hence a different entry point from `strerror` above.
pub(crate) fn multi_strerror(code: i32) -> Vec<u8> {
    read_sized(|ptr, cap, len| unsafe { elephc_curl_multi_strerror(code, ptr, cap, len) })
}

/// Detaches every still-attached easy handle and runs `curl_multi_cleanup`. A no-op for an
/// unknown/already-freed id.
pub(crate) fn multi_free(multi: i64) {
    unsafe { elephc_curl_multi_free(multi) }
}

/// Allocates a fresh share handle. `None` on libcurl allocation failure.
pub(crate) fn share_init() -> Option<i64> {
    let id = unsafe { elephc_curl_share_init() };
    (id != 0).then_some(id)
}

/// Finds (or creates) the PROCESS-LIFETIME share behind PHP 8.5's
/// `curl_share_init_persistent()`, keyed by the comma-separated decimal `CURL_LOCK_DATA_*`
/// list in `csv` — the same encoding `crate::curl_prelude::curl_share_init_persistent`
/// builds. `None` on libcurl allocation failure or a malformed list.
pub(crate) fn share_persistent_init(csv: &[u8]) -> Option<i64> {
    let id = unsafe { elephc_curl_share_persistent_init(csv.as_ptr(), csv.len()) };
    (id != 0).then_some(id)
}

/// Applies an integer-valued `CURLSHOPT_*` option, answering one of the `SHARE_SETOPT_*`
/// codes above.
pub(crate) fn share_setopt(share: i64, opt: i64, value: i64) -> i32 {
    unsafe { elephc_curl_share_setopt(share, opt, value) }
}

/// Returns the `CURLSHcode` from the most recent share operation (`curl_share_errno()`).
pub(crate) fn share_errno(share: i64) -> i64 {
    i64::from(unsafe { elephc_curl_share_errno(share) })
}

/// Returns libcurl's own message for a `CURLSHcode` — again its own numbering space.
pub(crate) fn share_strerror(code: i32) -> Vec<u8> {
    read_sized(|ptr, cap, len| unsafe { elephc_curl_share_strerror(code, ptr, cap, len) })
}

/// Points an easy handle's `CURLOPT_SHARE` at a share handle. `false` for an unknown id on
/// either side or a libcurl refusal.
pub(crate) fn easy_set_share(easy: i64, share: i64) -> bool {
    unsafe { elephc_curl_easy_set_share(easy, share) != 0 }
}

/// Requests that a share handle be released. DEFERRED inside the bridge while easy handles
/// are still attached, and a documented no-op for a persistent share — see
/// `crates/elephc-curl/src/share.rs`'s module doc.
pub(crate) fn share_free(share: i64) {
    unsafe { elephc_curl_share_free(share) }
}

/// LINK-SATISFYING STAND-INS for `cargo test -p elephc-magician --features curl`, NOT a
/// curl fake. `EvalDirectHook::call`/`EvalValuesHook::call` are single functions
/// containing ONE match over every registered builtin name (see
/// `crate::interpreter::builtins::curl`'s module doc for the full argument), so as soon
/// as `--features curl` compiles this crate's dispatch table in, it is unconditionally
/// "live" for the whole test binary — Rust cannot dead-strip individual match arms — and
/// every `elephc_curl_*` symbol above needs SOME definition to link, regardless of which
/// specific `#[test]` runs. Real `elephc-curl` is deliberately never linked into this
/// crate's own test binary (that would need real pinned libcurl/OpenSSL/zlib present on
/// every machine that runs `cargo test`, which is exactly what
/// `crates/elephc-curl/src/tests.rs`'s own `elephc_curl_native` gate exists to avoid).
/// These provide the bare minimum so the LINKER is satisfied; they do not implement any
/// libcurl semantic. Tests that need real curl behavior belong in the AOT
/// `tests/codegen/curl/` suite instead, gated by `skip_without_curl_native`.
#[cfg(test)]
mod test_stubs {
    /// Classifies the handful of options `crates/elephc-magician/src/stream_resources/
    /// tests` (the `CURLOPT_PRIVATE` retain/release regression) actually exercises,
    /// matching `crates/elephc-curl/src/options.rs`'s real table for exactly those
    /// entries; every other option answers `KIND_INVALID` (`0`), same as a genuinely
    /// unrecognized option — this stub does not attempt to reproduce the full table.
    #[no_mangle]
    extern "C" fn elephc_curl_option_kind(opt: i64) -> i32 {
        match opt {
            10002 => 2,  // CURLOPT_URL -> KIND_STRING
            10103 => 5,  // CURLOPT_PRIVATE -> KIND_PHP_LAYER
            19913 => 5,  // CURLOPT_RETURNTRANSFER -> KIND_PHP_LAYER
            _ => 0,      // KIND_INVALID
        }
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_init() -> i64 {
        // A fixed non-zero fake id: `curl_ffi::easy_init()` treats `0` as libcurl
        // allocation failure, and `curl_init()` (`crate::interpreter::builtins::curl::
        // curl_init`) hard-faults on that — tests need `curl_init()` to succeed so they
        // can reach `curl_setopt()`/`curl_getinfo()`'s own logic. Every eval `curl_init()`
        // call in a test gets its own independent `EvalStreamResources` table entry
        // regardless of this shared raw id, so reusing one value across handles is safe.
        42
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_set_url(_id: i64, _ptr: *const u8, _len: usize) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_setopt_long(_id: i64, _opt: i32, _value: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_setopt_str(
        _id: i64,
        _opt: i32,
        _ptr: *const u8,
        _len: usize,
    ) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_setopt_slist(
        _id: i64,
        _opt: i32,
        _ptr: *const u8,
        _len: usize,
    ) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_perform(_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_errno(_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_error(
        _id: i64,
        _out: *mut u8,
        _out_cap: usize,
        out_len: *mut usize,
    ) -> i32 {
        if !out_len.is_null() {
            unsafe {
                *out_len = 0;
            }
        }
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_getinfo_long(_id: i64, _info: i32, _out: *mut i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_getinfo_double(_id: i64, _info: i32, _out: *mut f64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_str_op(
        _id: i64,
        _op: i32,
        _ptr: *const u8,
        _len: usize,
        _number: i64,
    ) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_take_scratch(
        _id: i64,
        _ptr: *mut *mut u8,
        len: *mut usize,
    ) -> i32 {
        if !len.is_null() {
            unsafe {
                *len = 0;
            }
        }
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_take_body(
        _id: i64,
        _ptr: *mut *mut u8,
        len: *mut usize,
    ) -> i32 {
        if !len.is_null() {
            unsafe {
                *len = 0;
            }
        }
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_reset(_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_pause(_id: i64, _bitmask: i32) -> i32 {
        43
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_upkeep(_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_duphandle(_id: i64) -> i64 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_strerror(
        _code: i32,
        _out: *mut u8,
        _out_cap: usize,
        out_len: *mut usize,
    ) -> i32 {
        if !out_len.is_null() {
            unsafe {
                *out_len = 0;
            }
        }
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_free(_id: i64) {}
    #[no_mangle]
    extern "C" fn elephc_curl_global_info(_out_json: *mut u8, _cap: usize, len: *mut usize) -> i32 {
        if !len.is_null() {
            unsafe {
                *len = 0;
            }
        }
        0
    }
    /// A fixed non-zero fake id, for the same reason `elephc_curl_easy_init` returns one:
    /// `curl_multi_init()` hard-throws on `0`, and the eval-side table gives every call its
    /// own independent entry regardless of the shared raw id.
    #[no_mangle]
    extern "C" fn elephc_curl_multi_init() -> i64 {
        7
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_add(_multi_id: i64, _easy_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_remove(_multi_id: i64, _easy_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_perform(_multi_id: i64) -> i64 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_select(_multi_id: i64, _timeout_ms: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_info_read(_multi_id: i64, _field: i64) -> i64 {
        0
    }
    /// Classifies exactly the way `crates/elephc-curl/src/multi.rs`'s real
    /// `multi_option_kind` does for the handful of numbers the magician unit tests reach:
    /// the six `long` options apply, `CURLMOPT_PUSHFUNCTION` (20014) is a real-but-
    /// uncarryable option, everything else is not a multi option at all.
    #[no_mangle]
    extern "C" fn elephc_curl_multi_setopt(_multi_id: i64, opt: i64, _value: i64) -> i32 {
        match opt {
            3 | 6 | 7 | 8 | 13 | 16 | 30_009 | 30_010 => 1,
            20_014 => 0,
            _ => -1,
        }
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_errno(_multi_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_strerror(
        _code: i32,
        _out: *mut u8,
        _out_cap: usize,
        out_len: *mut usize,
    ) -> i32 {
        if !out_len.is_null() {
            unsafe {
                *out_len = 0;
            }
        }
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_multi_free(_multi_id: i64) {}
    #[no_mangle]
    extern "C" fn elephc_curl_share_init() -> i64 {
        11
    }
    #[no_mangle]
    extern "C" fn elephc_curl_share_persistent_init(_ptr: *const u8, _len: usize) -> i64 {
        13
    }
    /// `CURLSHOPT_SHARE` (1) and `CURLSHOPT_UNSHARE` (2) apply; every other number is
    /// php-src's own `ValueError`, matching `crates/elephc-curl/src/share.rs`'s
    /// `share_option_kind` for exactly those entries.
    #[no_mangle]
    extern "C" fn elephc_curl_share_setopt(_share_id: i64, opt: i64, _value: i64) -> i32 {
        match opt {
            1 | 2 => 1,
            _ => -1,
        }
    }
    #[no_mangle]
    extern "C" fn elephc_curl_share_errno(_share_id: i64) -> i32 {
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_share_strerror(
        _code: i32,
        _out: *mut u8,
        _out_cap: usize,
        out_len: *mut usize,
    ) -> i32 {
        if !out_len.is_null() {
            unsafe {
                *out_len = 0;
            }
        }
        0
    }
    #[no_mangle]
    extern "C" fn elephc_curl_easy_set_share(_easy_id: i64, _share_id: i64) -> i32 {
        1
    }
    #[no_mangle]
    extern "C" fn elephc_curl_share_free(_share_id: i64) {}
}
