//! Purpose:
//! Raw libcurl `extern "C"` bindings: the opaque `CURL` handle type, the
//! `CURLcode`/`CURLoption` numeric constants Task 3 needs, and thin typed
//! wrappers around the variadic `curl_easy_setopt`. Nothing here understands
//! PHP semantics (RETURNTRANSFER capture, stdout default) — see
//! `crate::php_layer` for that.
//!
//! Called from:
//! - `crate::abi` (the `#[no_mangle]` entry points) and `crate::php_layer`
//!   (the write callback and PHP-only-option dispatch).
//!
//! Key details:
//! - These symbols are declared, never linked, at `cargo build -p
//!   elephc-curl` time (see `build.rs` and `crates/elephc-curl/Cargo.toml`).
//! - `curl_easy_setopt` is a true C variadic function; libcurl expects the
//!   caller to pass whichever primitive type each `CURLOPT_*` document
//!   requires (`long`, a `const char *`, or a function/data pointer). The
//!   `setopt_*` wrappers below each call it with the matching Rust type so
//!   the C ABI sees the right argument shape.
//! - `CURLOPT_*` numeric values are frozen from `scripts/docs/curl_surface.json`
//!   (Task 1's PHP 8.2-8.5 surface extraction against this pinned libcurl),
//!   not recomputed from `CURLOPTTYPE_*` bases by hand.

use std::ffi::{c_char, c_int, c_long, c_void};

/// Opaque libcurl easy-handle type. Never constructed on the Rust side; only
/// ever seen as `*mut CURL` returned by `curl_easy_init`. Named `CURL`,
/// matching libcurl's own `curl.h` type name verbatim rather than
/// `Curl`/`CurlHandle`, so it reads as the same type when cross-referencing
/// the header.
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum CURL {}

/// libcurl's `CURLcode` return type. `0` is `CURLE_OK` (success); every other
/// value is a specific libcurl error.
pub(crate) type CURLcode = c_int;

/// `CURLE_OK`: the transfer/setopt succeeded.
pub(crate) const CURLE_OK: CURLcode = 0;

/// `CURL_ERROR_SIZE`: the minimum buffer size libcurl requires for
/// `CURLOPT_ERRORBUFFER`.
pub(crate) const CURL_ERROR_SIZE: usize = 256;

/// `CURLOPT_URL` (10002): the request URL. `CURLOPTTYPE_STRINGPOINT`.
pub(crate) const CURLOPT_URL: c_int = 10002;
/// `CURLOPT_ERRORBUFFER` (10010): a caller-owned `>= CURL_ERROR_SIZE` byte
/// buffer libcurl writes a human-readable error message into. Internal only;
/// never PHP-visible (PHP surfaces this through `curl_error()` instead).
pub(crate) const CURLOPT_ERRORBUFFER: c_int = 10010;
/// `CURLOPT_WRITEDATA` (10001, alias `CURLOPT_FILE`): the opaque pointer
/// handed back to the write callback's `userdata` parameter. Internal only;
/// elephc always sets this to the handle's own `i64` id.
pub(crate) const CURLOPT_WRITEDATA: c_int = 10001;
/// `CURLOPT_WRITEFUNCTION` (20011): the write-callback function pointer.
/// Internal only; elephc always installs its own callback (see
/// `crate::php_layer::write_callback`).
pub(crate) const CURLOPT_WRITEFUNCTION: c_int = 20011;

/// `CURLINFO_TYPEMASK`: the low bits of a `CURLINFO_*` value are irrelevant to
/// its C output type; `curl_easy_getinfo` dispatches purely on
/// `info & CURLINFO_TYPEMASK` (libcurl 8.21.0, `include/curl/curl.h`).
pub(crate) const CURLINFO_TYPEMASK: c_int = 0x00f0_0000;
/// `CURLINFO_LONG`: the type mask value for a `long`-typed info field (e.g.
/// `CURLINFO_RESPONSE_CODE`, PHP's `CURLINFO_HTTP_CODE`). `getinfo_long`
/// refuses to call libcurl for any `info` outside this type: `curl_easy_getinfo`
/// writes through its out-parameter according to this same type bit, the
/// read-side mirror of how `curl_easy_setopt` reads its variadic argument
/// according to `CURLOPTTYPE_*` (the wild-pointer hazard `src/curl_prelude.rs`
/// documents at length for `curl_setopt()` in the elephc root crate).
pub(crate) const CURLINFO_LONG: c_int = 0x0020_0000;

/// The `curl_write_callback` C function-pointer type:
/// `size_t (*)(char *buffer, size_t size, size_t nitems, void *outstream)`.
pub(crate) type CurlWriteCallback =
    unsafe extern "C" fn(*mut c_char, usize, usize, *mut c_void) -> usize;

/// `CURL_GLOBAL_DEFAULT` (`CURL_GLOBAL_ALL` = `CURL_GLOBAL_SSL | CURL_GLOBAL_WIN32`).
pub(crate) const CURL_GLOBAL_DEFAULT: c_long = 3;

/// `curl_version_info_data`, matching this pinned libcurl 8.21.0's
/// `CURLVERSION_TWELFTH` (`CURLVERSION_NOW`) struct shape field-for-field
/// (`include/curl/curl.h`). Only valid to read through the pointer
/// `curl_version_info` returns for `CURLVERSION_NOW`; libcurl owns that
/// storage (`'static`, never freed by the caller).
#[repr(C)]
pub(crate) struct CurlVersionInfoData {
    pub(crate) age: c_int,
    pub(crate) version: *const c_char,
    pub(crate) version_num: u32,
    pub(crate) host: *const c_char,
    pub(crate) features: c_int,
    pub(crate) ssl_version: *const c_char,
    pub(crate) ssl_version_num: c_long,
    pub(crate) libz_version: *const c_char,
    pub(crate) protocols: *const *const c_char,
    pub(crate) ares: *const c_char,
    pub(crate) ares_num: c_int,
    pub(crate) libidn: *const c_char,
    pub(crate) iconv_ver_num: c_int,
    pub(crate) libssh_version: *const c_char,
    pub(crate) brotli_ver_num: u32,
    pub(crate) brotli_version: *const c_char,
    pub(crate) nghttp2_ver_num: u32,
    pub(crate) nghttp2_version: *const c_char,
    pub(crate) quic_version: *const c_char,
    pub(crate) cainfo: *const c_char,
    pub(crate) capath: *const c_char,
    pub(crate) zstd_ver_num: u32,
    pub(crate) zstd_version: *const c_char,
    pub(crate) hyper_version: *const c_char,
    pub(crate) gsasl_version: *const c_char,
    pub(crate) feature_names: *const *const c_char,
    pub(crate) rtmp_version: *const c_char,
}

/// `CURLVERSION_NOW` (`CURLVERSION_TWELFTH` = 11 for this pinned libcurl).
pub(crate) const CURLVERSION_NOW: c_int = 11;

extern "C" {
    /// Must be called (successfully, exactly once per process) before any
    /// other libcurl entry point. See `crate::handles::ensure_global_init`.
    fn curl_global_init(flags: c_long) -> CURLcode;

    /// Allocates a new easy handle, or returns null on failure (out of
    /// memory).
    fn curl_easy_init() -> *mut CURL;

    /// Frees an easy handle and all resources libcurl attached to it.
    fn curl_easy_cleanup(curl: *mut CURL);

    /// The single variadic `curl_easy_setopt` symbol. Called only through
    /// the typed `setopt_*` wrappers below.
    fn curl_easy_setopt(curl: *mut CURL, option: c_int, ...) -> CURLcode;

    /// The single variadic `curl_easy_getinfo` symbol. Called only through
    /// the typed `getinfo_*` wrappers below.
    fn curl_easy_getinfo(curl: *mut CURL, info: c_int, ...) -> CURLcode;

    /// Runs the transfer configured on `curl`, blocking until it completes.
    fn curl_easy_perform(curl: *mut CURL) -> CURLcode;

    /// Returns the version/feature info struct for `stamp`
    /// (`CURLVERSION_NOW`). The returned pointer is libcurl-owned `'static`
    /// storage; never freed by the caller.
    fn curl_version_info(stamp: c_int) -> *const CurlVersionInfoData;
}

/// Ensures `curl_global_init` has run exactly once for this process. Every
/// libcurl entry point in this crate goes through this first; libcurl's own
/// contract requires global init before any other call and forbids calling
/// it more than once concurrently.
pub(crate) fn ensure_global_init() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        // Ignore a nonzero return: there is no recovery available (no easy
        // handle exists yet to report an error through), and every
        // documented failure mode (out of memory) will also fail the very
        // next libcurl call, which callers already handle.
        let _ = curl_global_init(CURL_GLOBAL_DEFAULT);
    });
}

/// Allocates a new libcurl easy handle after ensuring global init has run.
/// Returns null on libcurl allocation failure.
pub(crate) fn init() -> *mut CURL {
    ensure_global_init();
    unsafe { curl_easy_init() }
}

/// Frees a handle previously returned by [`init`].
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`] that has not already
/// been passed to this function.
pub(crate) unsafe fn cleanup(curl: *mut CURL) {
    curl_easy_cleanup(curl);
}

/// Runs the configured transfer to completion, returning the resulting
/// `CURLcode`.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn perform(curl: *mut CURL) -> CURLcode {
    curl_easy_perform(curl)
}

/// Sets a `long`-valued option (`CURLOPTTYPE_LONG`/`CURLOPTTYPE_VALUES`).
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn setopt_long(curl: *mut CURL, option: c_int, value: c_long) -> CURLcode {
    curl_easy_setopt(curl, option, value)
}

/// Sets a `curl_off_t`-valued option (`CURLOPTTYPE_OFF_T`, option numbers
/// 30000-39999).
///
/// This is a SEPARATE wrapper from [`setopt_long`] even though `c_long` and
/// `curl_off_t` are both 64-bit on every target elephc ships for (all LP64):
/// the variadic argument libcurl reads is typed `curl_off_t`, and naming that
/// explicitly here is what keeps the option-kind table's `KIND_OFF_T` rows
/// honest rather than relying on a coincidence of two typedefs.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn setopt_off_t(curl: *mut CURL, option: c_int, value: i64) -> CURLcode {
    curl_easy_setopt(curl, option, value)
}

/// Sets a NUL-terminated string option (`CURLOPTTYPE_STRINGPOINT`). `value`
/// must remain valid only for the duration of this call: libcurl copies
/// string options internally.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`]. `value` must be a
/// valid, NUL-terminated C string pointer.
pub(crate) unsafe fn setopt_str(curl: *mut CURL, option: c_int, value: *const c_char) -> CURLcode {
    curl_easy_setopt(curl, option, value)
}

/// Sets a raw data-pointer option (`CURLOPTTYPE_OBJECTPOINT`/`CBPOINT`), e.g.
/// `CURLOPT_ERRORBUFFER` or `CURLOPT_WRITEDATA`. Unlike string options,
/// libcurl stores this pointer verbatim and dereferences it later, so it must
/// stay valid for the handle's remaining lifetime.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`]. `value` must stay valid
/// for as long as `curl` may still use it (until cleanup or the option is
/// overwritten).
pub(crate) unsafe fn setopt_ptr(curl: *mut CURL, option: c_int, value: *mut c_void) -> CURLcode {
    curl_easy_setopt(curl, option, value)
}

/// Sets the write-callback function pointer (`CURLOPTTYPE_FUNCTIONPOINT`).
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn setopt_write_function(
    curl: *mut CURL,
    option: c_int,
    callback: CurlWriteCallback,
) -> CURLcode {
    curl_easy_setopt(curl, option, callback)
}

/// Reads a `long`-typed `curl_easy_getinfo` field (e.g. `CURLINFO_RESPONSE_CODE`,
/// PHP's `CURLINFO_HTTP_CODE`). Returns `None` for any `info` outside the
/// `CURLINFO_LONG` type range (never calls libcurl in that case — see
/// `CURLINFO_LONG`'s doc comment) or when libcurl itself reports a non-`CURLE_OK`
/// result.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn getinfo_long(curl: *mut CURL, info: c_int) -> Option<i64> {
    if info & CURLINFO_TYPEMASK != CURLINFO_LONG {
        return None;
    }
    let mut value: c_long = 0;
    let code = curl_easy_getinfo(curl, info, &mut value as *mut c_long);
    if code == CURLE_OK {
        Some(value as i64)
    } else {
        None
    }
}

/// Returns libcurl's version/feature info struct for `CURLVERSION_NOW`,
/// ensuring global init has run first. The returned reference borrows
/// libcurl's own `'static` storage.
pub(crate) fn version_info() -> &'static CurlVersionInfoData {
    ensure_global_init();
    unsafe {
        // `curl_version_info(CURLVERSION_NOW)` never returns null per libcurl's
        // own documentation (it is a simple accessor into static data).
        &*curl_version_info(CURLVERSION_NOW)
    }
}
