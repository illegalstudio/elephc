//! Purpose:
//! Raw libcurl `extern "C"` bindings: the opaque `CURL` handle type, the
//! `CURLcode`/`CURLoption` numeric constants the rest of this crate needs, and
//! thin typed wrappers around the variadic `curl_easy_setopt`. Nothing here understands
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
//!   (PHP 8.2-8.5's surface extracted against this pinned libcurl), not
//!   recomputed from `CURLOPTTYPE_*` bases by hand.

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
/// `CURLOPT_HTTPHEADER` (10023): the caller-owned request header list.
/// Monitoring may rebuild this list immediately before a transfer to add a
/// validated traceparent while preserving all user-provided headers.
pub(crate) const CURLOPT_HTTPHEADER: c_int = 10023;
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

/// `CURLOPT_POSTFIELDS` (10015): the request body. Never forwarded directly —
/// libcurl does NOT copy the buffer for this option, so elephc sets
/// [`CURLOPT_POSTFIELDSIZE_LARGE`] + [`CURLOPT_COPYPOSTFIELDS`] instead, which
/// is also what makes a body containing NUL bytes survive.
pub(crate) const CURLOPT_POSTFIELDS: c_int = 10015;
/// `CURLOPT_POSTFIELDSIZE_LARGE` (30120): the request body's exact byte length.
/// Must be set BEFORE `CURLOPT_COPYPOSTFIELDS`, which reads it to decide how
/// many bytes to copy (libcurl 8.21.0, `lib/setopt.c`); without it libcurl
/// falls back to `strlen`, truncating a binary body at its first NUL.
pub(crate) const CURLOPT_POSTFIELDSIZE_LARGE: c_int = 30120;
/// `CURLOPT_COPYPOSTFIELDS` (10165): like `CURLOPT_POSTFIELDS`, but libcurl
/// takes its own copy of the buffer, so the PHP string it came from is free to
/// die immediately.
pub(crate) const CURLOPT_COPYPOSTFIELDS: c_int = 10165;

/// `CURLOPT_SHARE` (10100): attaches a `CURLSH *` share object to this easy handle,
/// or detaches it when passed null. `crate::share` is the only writer of
/// this option; `crate::abi::elephc_curl_easy_duphandle` also clears it explicitly on
/// every duplicate (see that function's doc comment for why).
pub(crate) const CURLOPT_SHARE: c_int = 10100;

/// libcurl's opaque `struct curl_slist` linked-list node, built by
/// [`slist_append`] and freed by [`slist_free_all`]. Never constructed on the
/// Rust side; only ever seen as `*mut CurlSlist`.
pub(crate) enum CurlSlist {}

/// `CURLINFO_TYPEMASK`: the low bits of a `CURLINFO_*` value are irrelevant to
/// its C output type; `curl_easy_getinfo` dispatches purely on
/// `info & CURLINFO_TYPEMASK` (libcurl 8.21.0, `include/curl/curl.h`).
pub(crate) const CURLINFO_TYPEMASK: c_int = 0x00f0_0000;
/// `CURLINFO_STRING`: the type mask value for a `char *`-typed info field.
pub(crate) const CURLINFO_STRING: c_int = 0x0010_0000;
/// `CURLINFO_DOUBLE`: the type mask value for a `double`-typed info field.
pub(crate) const CURLINFO_DOUBLE: c_int = 0x0030_0000;
/// `CURLINFO_SLIST`: the type mask value for a `struct curl_slist *`-typed info
/// field. `CURLINFO_CERTINFO` shares this mask but returns a
/// `struct curl_certinfo *`, which is why [`getinfo_certinfo`] exists separately.
pub(crate) const CURLINFO_SLIST: c_int = 0x0040_0000;
/// `CURLINFO_OFF_T`: the type mask value for a `curl_off_t`-typed info field.
pub(crate) const CURLINFO_OFF_T: c_int = 0x0060_0000;
/// `CURLINFO_CERTINFO` (4194338): `CURLINFO_SLIST + 34`, but the out-parameter
/// is a `struct curl_certinfo *`, NOT a `struct curl_slist *`. Reading it
/// through the slist path would walk a `num_of_certs` integer as a list node.
pub(crate) const CURLINFO_CERTINFO: c_int = 0x0040_0000 + 34;

/// libcurl's `struct curl_certinfo`: a count plus an array of per-certificate
/// `struct curl_slist *`, each holding `"Name:Value"` lines. Owned by libcurl;
/// valid until the next transfer on the handle.
#[repr(C)]
pub(crate) struct CurlCertInfo {
    pub(crate) num_of_certs: c_int,
    pub(crate) certinfo: *mut *mut CurlSlist,
}

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

    /// Appends a NUL-terminated string to a `struct curl_slist`, returning the
    /// new head (or null on allocation failure, in which case the ORIGINAL
    /// list is leaked unless the caller kept its own pointer — which is why
    /// [`slist_append`] below never overwrites its input on failure).
    fn curl_slist_append(list: *mut CurlSlist, value: *const c_char) -> *mut CurlSlist;

    /// Frees an entire `struct curl_slist`. Safe on null.
    fn curl_slist_free_all(list: *mut CurlSlist);

    /// URL-encodes `len` bytes of `input` for this handle, returning a
    /// libcurl-allocated NUL-terminated string the caller frees with
    /// [`curl_free`], or null on allocation failure.
    fn curl_easy_escape(curl: *mut CURL, input: *const c_char, len: c_int) -> *mut c_char;

    /// URL-decodes `len` bytes of `input`, writing the decoded LENGTH through
    /// `outlength` (decoded data may contain NUL). The result is
    /// libcurl-allocated and freed with [`curl_free`].
    fn curl_easy_unescape(
        curl: *mut CURL,
        input: *const c_char,
        len: c_int,
        outlength: *mut c_int,
    ) -> *mut c_char;

    /// Frees memory libcurl allocated for the caller
    /// (`curl_easy_escape`/`curl_easy_unescape` results).
    fn curl_free(ptr: *mut c_char);

    /// Resets every option on `curl` to its default, keeping live connections,
    /// the session id cache, DNS cache and cookies. Callbacks and the error
    /// buffer are options, so they are reset too.
    fn curl_easy_reset(curl: *mut CURL);

    /// Pauses or unpauses the transfer on `curl` per the `CURLPAUSE_*` bitmask.
    fn curl_easy_pause(curl: *mut CURL, bitmask: c_int) -> CURLcode;

    /// Performs connection upkeep (keepalive pings) on `curl`'s idle
    /// connections.
    fn curl_easy_upkeep(curl: *mut CURL) -> CURLcode;

    /// Duplicates `curl` and every option set on it. The copy carries NO live
    /// connection, cookie, session or alt-svc state.
    fn curl_easy_duphandle(curl: *mut CURL) -> *mut CURL;

    /// Returns libcurl's `'static` human-readable text for a `CURLcode`.
    fn curl_easy_strerror(code: CURLcode) -> *const c_char;
}

/// URL-encodes `input` for `curl`, returning owned bytes. `None` when libcurl
/// could not allocate.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn escape(curl: *mut CURL, input: &[u8]) -> Option<Vec<u8>> {
    // EMPTY INPUT NEVER REACHES libcurl. `curl_easy_escape` documents length `0` as
    // "call strlen() on the input" — so passing the dangling (non-null, unallocated)
    // pointer Rust hands back for an empty slice makes libcurl read out of bounds.
    // Measured: a plain forward segfaulted on `curl_escape($ch, "")`.
    if input.is_empty() {
        return Some(Vec::new());
    }
    let Ok(len) = c_int::try_from(input.len()) else {
        return None;
    };
    let encoded = curl_easy_escape(curl, input.as_ptr() as *const c_char, len);
    if encoded.is_null() {
        return None;
    }
    let bytes = std::ffi::CStr::from_ptr(encoded).to_bytes().to_vec();
    curl_free(encoded);
    Some(bytes)
}

/// URL-decodes `input` for `curl`, returning owned bytes. The decoded result
/// may contain NUL bytes, so it is read by the LENGTH libcurl reports rather
/// than as a C string. `None` when libcurl could not allocate.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn unescape(curl: *mut CURL, input: &[u8]) -> Option<Vec<u8>> {
    // Same `strlen`-on-zero-length hazard as [`escape`] above.
    if input.is_empty() {
        return Some(Vec::new());
    }
    let Ok(len) = c_int::try_from(input.len()) else {
        return None;
    };
    let mut out_len: c_int = 0;
    let decoded = curl_easy_unescape(
        curl,
        input.as_ptr() as *const c_char,
        len,
        &mut out_len as *mut c_int,
    );
    if decoded.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(decoded as *const u8, out_len.max(0) as usize).to_vec();
    curl_free(decoded);
    Some(bytes)
}

/// Resets every option on `curl` to its default.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn reset(curl: *mut CURL) {
    curl_easy_reset(curl);
}

/// Applies a `CURLPAUSE_*` bitmask, returning libcurl's `CURLcode`.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn pause(curl: *mut CURL, bitmask: c_int) -> CURLcode {
    curl_easy_pause(curl, bitmask)
}

/// Runs connection upkeep, returning libcurl's `CURLcode`.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn upkeep(curl: *mut CURL) -> CURLcode {
    curl_easy_upkeep(curl)
}

/// Duplicates `curl` and its options. Null on allocation failure.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn duphandle(curl: *mut CURL) -> *mut CURL {
    curl_easy_duphandle(curl)
}

/// Returns libcurl's own message for a `CURLcode` as owned bytes.
pub(crate) fn strerror(code: CURLcode) -> Vec<u8> {
    ensure_global_init();
    unsafe {
        let text = curl_easy_strerror(code);
        if text.is_null() {
            Vec::new()
        } else {
            std::ffi::CStr::from_ptr(text).to_bytes().to_vec()
        }
    }
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

/// Sets a `struct curl_slist *` option (`CURLOPTTYPE_SLISTPOINT`).
///
/// UNLIKE string options, libcurl does NOT copy the list: it stores the pointer
/// and walks it during the transfer, so `list` must outlive every
/// `curl_easy_perform` on this handle. That lifetime is what
/// `crate::handles::EasyEntry::slists` exists to own.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`]. `list` must stay valid
/// until the option is replaced or the handle is cleaned up.
pub(crate) unsafe fn setopt_slist(
    curl: *mut CURL,
    option: c_int,
    list: *mut CurlSlist,
) -> CURLcode {
    curl_easy_setopt(curl, option, list)
}

/// Appends one NUL-terminated string to `list`, returning the new head, or
/// `None` when libcurl could not allocate — in which case `list` is UNCHANGED
/// and still the caller's to free.
///
/// # Safety
/// `list` must be null or a list this module built. `value` must be a valid,
/// NUL-terminated C string pointer.
pub(crate) unsafe fn slist_append(
    list: *mut CurlSlist,
    value: *const c_char,
) -> Option<*mut CurlSlist> {
    let next = curl_slist_append(list, value);
    if next.is_null() {
        None
    } else {
        Some(next)
    }
}

/// Frees a whole `struct curl_slist`. A no-op for null.
///
/// # Safety
/// `list` must be null or a list this module built that has not already been
/// freed, and must no longer be set on any live easy handle.
pub(crate) unsafe fn slist_free_all(list: *mut CurlSlist) {
    curl_slist_free_all(list);
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

/// Sets a `size_t (*)(char *, size_t, size_t, void *)`-shaped callback option —
/// `CURLOPT_HEADERFUNCTION` and `CURLOPT_READFUNCTION` share the write callback's C
/// signature. `None` writes a null function pointer, which is how libcurl restores the
/// option's built-in default.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn setopt_bytes_function(
    curl: *mut CURL,
    option: c_int,
    callback: Option<CurlWriteCallback>,
) -> CURLcode {
    match callback {
        Some(callback) => curl_easy_setopt(curl, option, callback),
        None => curl_easy_setopt(curl, option, std::ptr::null::<c_void>()),
    }
}

/// The `curl_xferinfo_callback` C function-pointer type:
/// `int (*)(void *clientp, curl_off_t dltotal, curl_off_t dlnow, curl_off_t ultotal,
/// curl_off_t ulnow)`. Backs BOTH of PHP's progress options — see
/// `crate::callbacks::xferinfo_callback` for why.
pub(crate) type CurlXferInfoCallback =
    unsafe extern "C" fn(*mut c_void, i64, i64, i64, i64) -> c_int;

/// Sets `CURLOPT_XFERINFOFUNCTION`. `None` clears it back to libcurl's default.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn setopt_xferinfo_function(
    curl: *mut CURL,
    option: c_int,
    callback: Option<CurlXferInfoCallback>,
) -> CURLcode {
    match callback {
        Some(callback) => curl_easy_setopt(curl, option, callback),
        None => curl_easy_setopt(curl, option, std::ptr::null::<c_void>()),
    }
}

/// The `curl_debug_callback` C function-pointer type:
/// `int (*)(CURL *handle, curl_infotype type, char *data, size_t size, void *clientp)`.
pub(crate) type CurlDebugCallback =
    unsafe extern "C" fn(*mut CURL, c_int, *mut c_char, usize, *mut c_void) -> c_int;

/// Sets `CURLOPT_DEBUGFUNCTION`. `None` clears it back to libcurl's default (which
/// writes verbose output to stderr when `CURLOPT_VERBOSE` is on).
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn setopt_debug_function(
    curl: *mut CURL,
    option: c_int,
    callback: Option<CurlDebugCallback>,
) -> CURLcode {
    match callback {
        Some(callback) => curl_easy_setopt(curl, option, callback),
        None => curl_easy_setopt(curl, option, std::ptr::null::<c_void>()),
    }
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
    // `CURLINFO_OFF_T` fields are accepted here too. They are a DIFFERENT C type
    // (`curl_off_t`) from `CURLINFO_LONG`'s `long`, but both are exactly 64 bits
    // on every target elephc supports (macos-aarch64, linux-aarch64,
    // linux-x86_64 — all LP64), so the same 8-byte out-parameter is correct for
    // both, and PHP surfaces both as `int`. This would need splitting if elephc
    // ever targeted an ILP32 platform.
    let type_mask = info & CURLINFO_TYPEMASK;
    if type_mask != CURLINFO_LONG && type_mask != CURLINFO_OFF_T {
        return None;
    }
    let mut value: i64 = 0;
    let code = curl_easy_getinfo(curl, info, &mut value as *mut i64);
    if code == CURLE_OK {
        Some(value)
    } else {
        None
    }
}

/// Reads a `double`-typed `curl_easy_getinfo` field (`CURLINFO_TOTAL_TIME`,
/// `CURLINFO_SPEED_DOWNLOAD`, ...). Returns `None` for any `info` outside the
/// `CURLINFO_DOUBLE` type range or a libcurl failure — the same type-mask guard
/// [`getinfo_long`] documents, for the same reason.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn getinfo_double(curl: *mut CURL, info: c_int) -> Option<f64> {
    if info & CURLINFO_TYPEMASK != CURLINFO_DOUBLE {
        return None;
    }
    let mut value: f64 = 0.0;
    let code = curl_easy_getinfo(curl, info, &mut value as *mut f64);
    if code == CURLE_OK {
        Some(value)
    } else {
        None
    }
}

/// Reads a `char *`-typed `curl_easy_getinfo` field into an owned byte vector.
///
/// THE TWO "NO VALUE" CASES ARE DIFFERENT AND BOTH MATTER, which is why the
/// result is doubly optional:
/// - `None` — the type mask was wrong, or libcurl itself reported a failure.
/// - `Some(None)` — libcurl succeeded and handed back a NULL `char *`, i.e. this
///   transfer has no value for the field. php-src answers `false` for that in the
///   typed `curl_getinfo($ch, CURLINFO_*)` form and an explicit `null` for
///   `content_type` in the array form, so collapsing it into an empty string
///   would report a value PHP never reports.
/// - `Some(Some(bytes))` — a real value, possibly legitimately empty.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn getinfo_str(curl: *mut CURL, info: c_int) -> Option<Option<Vec<u8>>> {
    if info & CURLINFO_TYPEMASK != CURLINFO_STRING {
        return None;
    }
    let mut value: *const c_char = std::ptr::null();
    let code = curl_easy_getinfo(curl, info, &mut value as *mut *const c_char);
    if code != CURLE_OK {
        return None;
    }
    if value.is_null() {
        return Some(None);
    }
    Some(Some(
        std::ffi::CStr::from_ptr(value).to_bytes().to_vec(),
    ))
}

/// Reads a `struct curl_slist *`-typed `curl_easy_getinfo` field
/// (`CURLINFO_COOKIELIST`, `CURLINFO_SSL_ENGINES`) into owned byte vectors.
///
/// `CURLINFO_COOKIELIST` hands back a list the CALLER must free; the others hand
/// back libcurl-owned storage. libcurl documents `curl_slist_free_all` on the
/// cookie list specifically, so [`getinfo_cookielist`] wraps that case and this
/// one never frees.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn getinfo_slist(curl: *mut CURL, info: c_int) -> Option<Vec<Vec<u8>>> {
    if info & CURLINFO_TYPEMASK != CURLINFO_SLIST || info == CURLINFO_CERTINFO {
        return None;
    }
    let mut list: *mut CurlSlist = std::ptr::null_mut();
    let code = curl_easy_getinfo(curl, info, &mut list as *mut *mut CurlSlist);
    if code != CURLE_OK {
        return None;
    }
    let items = read_slist(list);
    // `CURLINFO_COOKIELIST` is the one slist info libcurl hands to the caller to
    // free (`curl_easy_getinfo` docs); the rest point into libcurl's own state.
    if info == CURLINFO_COOKIELIST {
        slist_free_all(list);
    }
    Some(items)
}

/// `CURLINFO_COOKIELIST` (4194332): the one `CURLINFO_SLIST` field whose list
/// libcurl transfers to the caller, so it is also the one this crate frees.
pub(crate) const CURLINFO_COOKIELIST: c_int = 0x0040_0000 + 28;

/// Reads `CURLINFO_CERTINFO` into per-certificate `"Name:Value"` line lists.
/// Returns `None` when libcurl has no certificate chain (a plaintext transfer,
/// or `CURLOPT_CERTINFO` never enabled) or reports a failure.
///
/// # Safety
/// `curl` must be a still-live pointer from [`init`].
pub(crate) unsafe fn getinfo_certinfo(curl: *mut CURL) -> Option<Vec<Vec<Vec<u8>>>> {
    let mut info: *mut CurlCertInfo = std::ptr::null_mut();
    let code = curl_easy_getinfo(curl, CURLINFO_CERTINFO, &mut info as *mut *mut CurlCertInfo);
    if code != CURLE_OK || info.is_null() {
        return None;
    }
    let count = (*info).num_of_certs;
    if count <= 0 || (*info).certinfo.is_null() {
        return Some(Vec::new());
    }
    let mut certs = Vec::with_capacity(count as usize);
    for index in 0..count as isize {
        certs.push(read_slist(*(*info).certinfo.offset(index)));
    }
    Some(certs)
}

/// Walks a `struct curl_slist` into owned byte vectors without freeing it.
///
/// Also used by `crate::abi::elephc_curl_easy_duphandle` to REBUILD a handle's list
/// options for the copy: `curl_easy_duphandle` shares them rather than duplicating them
/// (libcurl 8.21.0, `lib/easy.c`'s `dupset` — `dst->set = src->set` carries every
/// `struct curl_slist *` across verbatim), so the copy needs lists of its own.
///
/// # Safety
/// `list` must be null or a valid libcurl slist for the duration of the call.
pub(crate) unsafe fn read_slist(list: *mut CurlSlist) -> Vec<Vec<u8>> {
    // `struct curl_slist { char *data; struct curl_slist *next; }` — two
    // pointers, in that order, in every libcurl release. Read through a local
    // `#[repr(C)]` mirror rather than by pointer arithmetic so the layout is
    // stated once and checked by the compiler.
    #[repr(C)]
    struct SlistNode {
        data: *mut c_char,
        next: *mut SlistNode,
    }
    let mut items = Vec::new();
    let mut node = list as *mut SlistNode;
    while !node.is_null() {
        if !(*node).data.is_null() {
            items.push(std::ffi::CStr::from_ptr((*node).data).to_bytes().to_vec());
        } else {
            items.push(Vec::new());
        }
        node = (*node).next;
    }
    items
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
