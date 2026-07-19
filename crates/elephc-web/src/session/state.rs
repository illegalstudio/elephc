//! Purpose:
//! Owns the per-worker PHP session bridge state statics (name, ID, status, save
//! path, cache limiter/expire, cookie parameters, strict/serialize/GC/SID
//! configuration, the data snapshot used by `session_reset`/`session_abort`, and
//! the held session-file descriptor) plus the shared string-buffer helpers every
//! other session submodule uses to return C strings. Provides the state
//! getters/setters and the per-request reset entry point.
//!
//! Called from:
//! - The compiled `--web` web prelude, which declares each
//!   `elephc_web_session_*` symbol as `extern "elephc_web"` and calls them from
//!   the PHP-level `session_*()` function implementations.
//! - Sibling session submodules (`file_io`, `id`, `wire_format`), which reuse
//!   the `RET_STRING`/`SESSION_FD`/`SESSION_SNAPSHOT`/`SESSION_ID` statics and
//!   the `opt_ptr`/`set_cstr`/`cstr_to_string` string-buffer helpers declared
//!   here.
//!
//! Key details:
//! - One process per prefork worker, single-threaded: each request runs to
//!   completion on the worker's one thread, so all process-statics are race-free
//!   (same invariant as `request_state.rs`).
//! - All access to `static mut` items goes through raw pointers
//!   (`core::ptr::addr_of_mut!` / `core::ptr::addr_of!`), never `&mut`/`&`
//!   references, to stay clear of the `static_mut_refs` lint.
//! - String returns use per-worker `Option<CString>` statics; the compiler copies
//!   the bytes immediately after the call, so the buffer only needs to survive
//!   until the next session C-ABI call.
//! - Statics and helpers here are `pub(super)` so sibling submodules under
//!   `session` can reach them directly; nothing outside `session` may touch
//!   them.

use std::ffi::{c_char, CString};
use std::sync::OnceLock;

// ── Session state (per-worker process statics) ──

/// Session name (cookie name). Default: `"PHPSESSID"`.
pub(super) static mut SESSION_NAME: Option<CString> = None;
/// Current session ID. Default: empty (none).
pub(super) static mut SESSION_ID: Option<CString> = None;
/// Session status: 0=disabled, 1=none, 2=active. Default: 1 (PHP_SESSION_NONE).
pub(super) static mut SESSION_STATUS: i64 = 1;
/// Session save path. Default: the system temp directory.
pub(super) static mut SESSION_SAVE_PATH: Option<CString> = None;
/// Cache limiter string. Default: `"nocache"`.
pub(super) static mut SESSION_CACHE_LIMITER: Option<CString> = None;
/// Cache expire in minutes. Default: 180.
pub(super) static mut SESSION_CACHE_EXPIRE: i64 = 180;
/// Cookie lifetime in seconds. Default: 0 (session cookie).
pub(super) static mut COOKIE_LIFETIME: i64 = 0;
/// Cookie path. Default: `"/"`.
pub(super) static mut COOKIE_PATH: Option<CString> = None;
/// Cookie domain. Default: empty.
pub(super) static mut COOKIE_DOMAIN: Option<CString> = None;
/// Cookie secure flag. Default: false.
pub(super) static mut COOKIE_SECURE: bool = false;
/// Cookie Partitioned flag. Default: false.
pub(super) static mut COOKIE_PARTITIONED: bool = false;
/// Cookie httponly flag. Default: false, matching php-src 8.2 through 8.5.
pub(super) static mut COOKIE_HTTPONLY: bool = false;
/// Cookie SameSite attribute. Default: empty.
pub(super) static mut COOKIE_SAMESITE: Option<CString> = None;
/// Data snapshot for `session_reset`/`session_abort` (the original file content).
pub(super) static mut SESSION_SNAPSHOT: Vec<u8> = Vec::new();
/// Held session-file descriptor (`-1` = none). Kept open with `flock(LOCK_EX)`
/// between `session_read` and `session_write`/`session_destroy`/`session_abort`.
pub(super) static mut SESSION_FD: i32 = -1;

// ── Extended session configuration (v3 additions) ──

/// Strict session-ID mode flag (`session.use_strict_mode`): when non-zero,
/// `session_start` rejects a client-supplied ID whose file does not already
/// exist and mints a fresh one instead (anti session-fixation). Default: 0.
pub(super) static mut STRICT_MODE: i64 = 0;
/// Serialize handler name (`session.serialize_handler`): `"php"`,
/// `"php_serialize"`, or `"php_binary"`. Default: `"php"`.
pub(super) static mut SERIALIZE_HANDLER: Option<CString> = None;
/// GC invocation probability numerator (`session.gc_probability`). Default: 1.
pub(super) static mut GC_PROBABILITY: i64 = 1;
/// GC invocation probability denominator (`session.gc_divisor`). Default: 100.
pub(super) static mut GC_DIVISOR: i64 = 100;
/// Session max lifetime in seconds before GC deletes a file
/// (`session.gc_maxlifetime`). Default: 1440.
pub(super) static mut GC_MAXLIFETIME: i64 = 1440;
/// Generated session ID length in characters (`session.sid_length`).
/// Default: 32. Valid range enforced by the setter: `[22, 256]`.
pub(super) static mut SID_LENGTH: i64 = 32;
/// Bits of entropy encoded per generated ID character
/// (`session.sid_bits_per_character`). Default: 4 (charset `0-9a-f`). Valid
/// values enforced by the setter: 4 (`0-9a-f`), 5 (`0-9a-v`), 6
/// (`0-9a-zA-Z,-`).
pub(super) static mut SID_BITS_PER_CHARACTER: i64 = 4;

// ── Session ini config layer (v4 additions) ──
//
// These mirror the remaining PHP `session.*` ini directives so they are
// reachable through `ini_get`/`ini_set`/`ini_get_all` and the `session_start()`
// options array. Only their config storage lives here; the request-time server
// behaviour they gate (URL-rewriting for `use_trans_sid`, the upload-progress
// tracker) is owned by later server-side work, not this config layer.

/// Referer-check substring (`session.referer_check`): when non-empty,
/// `session_start` rejects a cookie-supplied ID whose request `Referer` header
/// does not contain this substring (anti session-fixation). Default: `""`.
pub(super) static mut SESSION_REFERER_CHECK: Option<CString> = None;
/// `session.use_cookies`: accept and emit session cookies. Default: 1.
pub(super) static mut USE_COOKIES: i64 = 1;
/// `session.use_only_cookies`: when 1, the session ID is only ever read from
/// the cookie (never the URL). Default: 1.
pub(super) static mut USE_ONLY_COOKIES: i64 = 1;
/// `session.lazy_write`: timestamp unchanged data instead of rewriting it.
/// Default: 1.
pub(super) static mut LAZY_WRITE: i64 = 1;
/// `session.use_trans_sid`: when 1, the ID is propagated through URL rewriting
/// for cookie-less clients. Config storage only; the rewriter lands later.
/// Default: 0.
pub(super) static mut USE_TRANS_SID: i64 = 0;
/// `session.trans_sid_tags`: the HTML tag/attribute pairs the URL rewriter
/// targets. Default: `"a=href,area=href,frame=src,form="`.
pub(super) static mut TRANS_SID_TAGS: Option<CString> = None;
/// `session.trans_sid_hosts`: the extra hosts URL rewriting is allowed to
/// target. Default: `""`.
pub(super) static mut TRANS_SID_HOSTS: Option<CString> = None;
/// `session.upload_progress.enabled`: whether upload progress tracking is on.
/// Config storage only; the tracker lands later. Default: 1.
pub(super) static mut UPLOAD_PROGRESS_ENABLED: i64 = 1;
/// `session.upload_progress.cleanup`: whether the progress entry is removed
/// once the upload completes. Default: 1.
pub(super) static mut UPLOAD_PROGRESS_CLEANUP: i64 = 1;
/// `session.upload_progress.prefix`: the `$_SESSION` key prefix for progress
/// entries. Default: `"upload_progress_"`.
pub(super) static mut UPLOAD_PROGRESS_PREFIX: Option<CString> = None;
/// `session.upload_progress.name`: the form field name that triggers progress
/// tracking. Default: `"PHP_SESSION_UPLOAD_PROGRESS"`.
pub(super) static mut UPLOAD_PROGRESS_NAME: Option<CString> = None;
/// `session.upload_progress.freq`: how often the progress entry is updated
/// (bytes or a `%` of total). Default: `"1%"`.
pub(super) static mut UPLOAD_PROGRESS_FREQ: Option<CString> = None;
/// `session.upload_progress.min_freq`: minimum seconds between progress
/// updates. Default: `"1"`.
pub(super) static mut UPLOAD_PROGRESS_MIN_FREQ: Option<CString> = None;

/// `session.auto_start` per-request working copy: when 1, the web prelude calls
/// `session_start()` automatically after the superglobals are built. Reset each
/// request from the process-level [`auto_start_config`] (the php.ini-PERDIR
/// analog), so a mid-request `ini_set` does not leak into the next request.
/// Default: seeded from `ELEPHC_SESSION_AUTO_START`.
pub(super) static mut SESSION_AUTO_START: i64 = 0;

/// Process-level `session.auto_start` config, read once per worker process from
/// the `ELEPHC_SESSION_AUTO_START` environment variable. This is the immutable
/// startup value (php.ini PERDIR analog); the per-request working copy lives in
/// [`SESSION_AUTO_START`].
static AUTO_START_CONFIG: OnceLock<i64> = OnceLock::new();

/// Returns the process-level `session.auto_start` config, parsing
/// `ELEPHC_SESSION_AUTO_START` on first call. `"1"`, `"on"`, or `"true"`
/// (case-insensitive, trimmed) map to 1; anything else (including an unset
/// variable) maps to 0. Single-threaded per worker, so the lazy init cannot
/// race.
fn auto_start_config() -> i64 {
    *AUTO_START_CONFIG.get_or_init(|| match std::env::var("ELEPHC_SESSION_AUTO_START") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            if v == "1" || v == "on" || v == "true" {
                1
            } else {
                0
            }
        }
        Err(_) => 0,
    })
}

// ── Return-string buffers (per-worker, valid until next session call) ──

/// Buffer for `session_get_name` / `session_read` / `session_create_id` /
/// `session_entry_key` / `session_entry_value` string returns.
pub(super) static mut RET_STRING: Option<CString> = None;

/// Binary-safe transfer buffer for serialized session payloads and parsed
/// wire-format entries. PHP accesses it through pointer builtins plus an
/// explicit byte length, so embedded NUL bytes are never truncated.
pub(super) static mut DATA_BUFFER: Vec<u8> = Vec::new();

/// Request-scoped owners for byte payloads already returned to generated PHP.
/// Pointer strings are borrowed by the compiler, so a later bridge call must
/// not invalidate an earlier result while session decoding still uses it.
static mut PUBLISHED_BUFFERS: Vec<Box<[u8]>> = Vec::new();

/// Writable inbound staging storage used by generated PHP before calling a
/// byte-oriented session bridge function. This must remain distinct from
/// `DATA_BUFFER`: a PHP string returned by `publish_bytes` may still borrow the
/// outbound buffer when the next bridge call stages that same string.
static mut STAGING_BUFFER: Vec<u8> = Vec::new();

/// Publishes bytes in the shared transfer buffer and returns their pointer as
/// an integer accepted by elephc's `ptr_read_string` builtin.
pub(super) unsafe fn publish_bytes(bytes: &[u8]) -> i64 {
    let buffer = &mut *core::ptr::addr_of_mut!(DATA_BUFFER);
    buffer.clear();
    buffer.extend_from_slice(bytes);
    if buffer.is_empty() {
        0
    } else {
        let published = &mut *core::ptr::addr_of_mut!(PUBLISHED_BUFFERS);
        published.push(buffer.clone().into_boxed_slice());
        published.last().expect("published payload was just pushed").as_ptr() as i64
    }
}

/// Borrows an explicit pointer/length payload supplied by generated PHP code.
/// A null pointer or non-positive length represents an empty byte string.
pub(super) unsafe fn input_bytes<'a>(ptr: *const u8, len: i64) -> &'a [u8] {
    if ptr.is_null() || len <= 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len as usize)
    }
}

/// Returns the C-string pointer held in an `Option<CString>` static, or an empty
/// string pointer when unset. The pointer is valid until the static is next
/// written — the compiler copies it immediately after the call.
pub(super) unsafe fn opt_ptr(slot: *const Option<CString>) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match &*slot {
        Some(s) => s.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Stores a Rust string into a `Option<CString>` static via raw pointer, replacing
/// any prior value. Interior NULs are stripped (CString cannot hold them).
pub(super) unsafe fn set_cstr(slot: *mut Option<CString>, s: &str) {
    let cstr = CString::new(s.replace('\0', "")).unwrap_or_default();
    core::ptr::write(slot, Some(cstr));
}

/// Converts a NUL-terminated C string pointer into an owned Rust string.
/// Returns an empty string for a null pointer.
pub(super) unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

// ═══════════════════════════════════════════════════════════════════════════
// Session state getters/setters
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the current session name (cookie name) as a NUL-terminated C string.
/// Valid until the next session C-ABI call. Default: `"PHPSESSID"`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_name() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(SESSION_NAME);
    if (*slot).is_none() {
        set_cstr(slot, "PHPSESSID");
    }
    opt_ptr(core::ptr::addr_of!(SESSION_NAME))
}

/// Sets the session name (cookie name) from a NUL-terminated C string.
/// Called by the prelude's `session_name()` setter.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_name(ptr: *const c_char) {
    set_cstr(core::ptr::addr_of_mut!(SESSION_NAME), &cstr_to_string(ptr));
}

/// Returns the current session ID as a NUL-terminated C string, or empty string
/// when no ID is set. Valid until the next session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_id() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(SESSION_ID))
}

/// Sets the session ID from a NUL-terminated C string. Returns 1 on success.
/// Called by the prelude's `session_id()` setter.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_id(ptr: *const c_char) -> i64 {
    set_cstr(core::ptr::addr_of_mut!(SESSION_ID), &cstr_to_string(ptr));
    1
}

/// Returns the current session status: 0=disabled, 1=none, 2=active.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_status() -> i64 {
    *core::ptr::addr_of!(SESSION_STATUS)
}

/// Sets the session status. Called by the prelude to transition between
/// `PHP_SESSION_NONE` and `PHP_SESSION_ACTIVE`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_status(status: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_STATUS), status);
}

/// Returns the configured session save path as a NUL-terminated C string.
/// The empty php-src default is resolved to the system temp directory only by
/// the files handler, not by this configuration getter.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_save_path() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(SESSION_SAVE_PATH);
    if (*slot).is_none() {
        set_cstr(slot, "");
    }
    opt_ptr(core::ptr::addr_of!(SESSION_SAVE_PATH))
}

/// Sets the session save path from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_save_path(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(SESSION_SAVE_PATH),
        &cstr_to_string(ptr),
    );
}

/// Resizes the binary transfer buffer for an inbound PHP string and returns a
/// writable pointer which the caller immediately fills with `ptr_write_string`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_data_stage(len: i64) -> i64 {
    let buffer = &mut *core::ptr::addr_of_mut!(STAGING_BUFFER);
    buffer.resize(len.max(0) as usize, 0);
    if buffer.is_empty() {
        0
    } else {
        buffer.as_mut_ptr() as i64
    }
}

/// Returns the byte length currently held in the binary transfer buffer.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_data_len() -> i64 {
    (*core::ptr::addr_of!(DATA_BUFFER)).len() as i64
}

/// Returns the cache limiter string (default: `"nocache"`). Valid until the next
/// session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cache_limiter() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(SESSION_CACHE_LIMITER);
    if (*slot).is_none() {
        set_cstr(slot, "nocache");
    }
    opt_ptr(core::ptr::addr_of!(SESSION_CACHE_LIMITER))
}

/// Sets the cache limiter from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_cache_limiter(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(SESSION_CACHE_LIMITER),
        &cstr_to_string(ptr),
    );
}

/// Returns the cache expire in minutes (default: 180).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cache_expire() -> i64 {
    *core::ptr::addr_of!(SESSION_CACHE_EXPIRE)
}

/// Sets the cache expire in minutes.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_cache_expire(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_CACHE_EXPIRE), v);
}

// ═══════════════════════════════════════════════════════════════════════════
// Cookie params getters/setters
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the cookie lifetime in seconds (default: 0 = session cookie).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_lifetime() -> i64 {
    *core::ptr::addr_of!(COOKIE_LIFETIME)
}

/// Returns the cookie path as a NUL-terminated C string (default: `"/"`).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_path() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(COOKIE_PATH);
    if (*slot).is_none() {
        set_cstr(slot, "/");
    }
    opt_ptr(core::ptr::addr_of!(COOKIE_PATH))
}

/// Returns the cookie domain as a NUL-terminated C string (default: empty).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_domain() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(COOKIE_DOMAIN);
    if (*slot).is_none() {
        set_cstr(slot, "");
    }
    opt_ptr(core::ptr::addr_of!(COOKIE_DOMAIN))
}

/// Returns 1 if the cookie secure flag is set, 0 otherwise (default: 0).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_secure() -> i64 {
    if *core::ptr::addr_of!(COOKIE_SECURE) {
        1
    } else {
        0
    }
}

/// Returns 1 when the session cookie carries the Partitioned attribute.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_partitioned() -> i64 {
    if *core::ptr::addr_of!(COOKIE_PARTITIONED) {
        1
    } else {
        0
    }
}

/// Returns 1 if the cookie httponly flag is set, 0 otherwise (default: 0).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_httponly() -> i64 {
    if *core::ptr::addr_of!(COOKIE_HTTPONLY) {
        1
    } else {
        0
    }
}

/// Returns the cookie SameSite attribute as a NUL-terminated C string
/// (default: empty).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_samesite() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(COOKIE_SAMESITE);
    if (*slot).is_none() {
        set_cstr(slot, "");
    }
    opt_ptr(core::ptr::addr_of!(COOKIE_SAMESITE))
}

/// Sets all seven cookie parameters at once. String parameters are NUL-terminated
/// C strings; boolean options are `i64` flags (0=false, non-zero=true).
/// Called by the prelude's `session_set_cookie_params()`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_cookie_params(
    lifetime: i64,
    path_ptr: *const c_char,
    domain_ptr: *const c_char,
    secure: i64,
    partitioned: i64,
    httponly: i64,
    samesite_ptr: *const c_char,
) {
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_LIFETIME), lifetime);
    set_cstr(
        core::ptr::addr_of_mut!(COOKIE_PATH),
        &cstr_to_string(path_ptr),
    );
    set_cstr(
        core::ptr::addr_of_mut!(COOKIE_DOMAIN),
        &cstr_to_string(domain_ptr),
    );
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_SECURE), secure != 0);
    core::ptr::write(
        core::ptr::addr_of_mut!(COOKIE_PARTITIONED),
        partitioned != 0,
    );
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_HTTPONLY), httponly != 0);
    set_cstr(
        core::ptr::addr_of_mut!(COOKIE_SAMESITE),
        &cstr_to_string(samesite_ptr),
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Strict mode / serialize handler / GC / SID config getters/setters (v3)
// ═══════════════════════════════════════════════════════════════════════════

/// Returns 1 if strict session-ID mode (`session.use_strict_mode`) is
/// enabled, 0 otherwise (default: 0).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_strict_mode() -> i64 {
    *core::ptr::addr_of!(STRICT_MODE)
}

/// Sets strict session-ID mode. Non-zero enables anti-fixation rejection of
/// client-supplied IDs whose file does not already exist.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_strict_mode(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(STRICT_MODE), v);
}

/// Returns the serialize handler name as a NUL-terminated C string
/// (default: `"php"`). Valid until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_serialize_handler() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(SERIALIZE_HANDLER);
    if (*slot).is_none() {
        set_cstr(slot, "php");
    }
    opt_ptr(core::ptr::addr_of!(SERIALIZE_HANDLER))
}

/// Sets the serialize handler name (`"php"`, `"php_serialize"`, or
/// `"php_binary"`) from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_serialize_handler(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(SERIALIZE_HANDLER),
        &cstr_to_string(ptr),
    );
}

/// Returns the GC invocation probability numerator (default: 1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_gc_probability() -> i64 {
    *core::ptr::addr_of!(GC_PROBABILITY)
}

/// Sets the GC invocation probability numerator.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_gc_probability(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(GC_PROBABILITY), v);
}

/// Returns the GC invocation probability denominator (default: 100).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_gc_divisor() -> i64 {
    *core::ptr::addr_of!(GC_DIVISOR)
}

/// Sets the GC invocation probability denominator.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_gc_divisor(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(GC_DIVISOR), v);
}

/// Returns the session max lifetime in seconds before GC deletes a file
/// (default: 1440).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_gc_maxlifetime() -> i64 {
    *core::ptr::addr_of!(GC_MAXLIFETIME)
}

/// Sets the session max lifetime in seconds before GC deletes a file.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_gc_maxlifetime(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(GC_MAXLIFETIME), v);
}

/// Returns the configured generated session ID length (default: 32).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_sid_length() -> i64 {
    *core::ptr::addr_of!(SID_LENGTH)
}

/// Sets the generated session ID length. Rejects (returns 0, state
/// unchanged) values outside `[22, 256]`, matching PHP's own ini validation
/// for `session.sid_length`. Returns 1 on success.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_sid_length(v: i64) -> i64 {
    if !(22..=256).contains(&v) {
        return 0;
    }
    core::ptr::write(core::ptr::addr_of_mut!(SID_LENGTH), v);
    1
}

/// Returns the configured bits of entropy per generated ID character
/// (default: 4).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_sid_bits_per_character() -> i64 {
    *core::ptr::addr_of!(SID_BITS_PER_CHARACTER)
}

/// Sets the bits of entropy per generated ID character. Rejects (returns 0,
/// state unchanged) values outside `[4, 6]` — the only charsets PHP defines
/// for `session.sid_bits_per_character` (4 -> `0-9a-f`, 5 -> `0-9a-v`,
/// 6 -> `0-9a-zA-Z,-`). Returns 1 on success.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_sid_bits_per_character(v: i64) -> i64 {
    if !(4..=6).contains(&v) {
        return 0;
    }
    core::ptr::write(core::ptr::addr_of_mut!(SID_BITS_PER_CHARACTER), v);
    1
}

// ═══════════════════════════════════════════════════════════════════════════
// Session ini config layer getters/setters (v4)
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the referer-check substring (`session.referer_check`, default `""`)
/// as a NUL-terminated C string. Valid until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_referer_check() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(SESSION_REFERER_CHECK);
    if (*slot).is_none() {
        set_cstr(slot, "");
    }
    opt_ptr(core::ptr::addr_of!(SESSION_REFERER_CHECK))
}

/// Sets the referer-check substring (`session.referer_check`) from a
/// NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_referer_check(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(SESSION_REFERER_CHECK),
        &cstr_to_string(ptr),
    );
}

/// Returns the `session.use_only_cookies` flag (default: 1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_use_only_cookies() -> i64 {
    *core::ptr::addr_of!(USE_ONLY_COOKIES)
}

/// Sets the `session.use_only_cookies` flag.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_use_only_cookies(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(USE_ONLY_COOKIES), v);
}

/// Returns the `session.use_cookies` flag (default: 1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_use_cookies() -> i64 {
    *core::ptr::addr_of!(USE_COOKIES)
}

/// Sets the `session.use_cookies` flag.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_use_cookies(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(USE_COOKIES), v);
}

/// Returns the `session.lazy_write` flag (default: 1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_lazy_write() -> i64 {
    *core::ptr::addr_of!(LAZY_WRITE)
}

/// Sets the `session.lazy_write` flag.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_lazy_write(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(LAZY_WRITE), v);
}

/// Returns the `session.use_trans_sid` flag (default: 0).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_use_trans_sid() -> i64 {
    *core::ptr::addr_of!(USE_TRANS_SID)
}

/// Sets the `session.use_trans_sid` flag. Config storage only — the URL
/// rewriter that consumes it lands in later server-side work.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_use_trans_sid(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(USE_TRANS_SID), v);
}

/// Returns the `session.trans_sid_tags` string (default:
/// `"a=href,area=href,frame=src,form="`). Valid until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_trans_sid_tags() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(TRANS_SID_TAGS);
    if (*slot).is_none() {
        set_cstr(slot, "a=href,area=href,frame=src,form=");
    }
    opt_ptr(core::ptr::addr_of!(TRANS_SID_TAGS))
}

/// Sets the `session.trans_sid_tags` string from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_trans_sid_tags(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(TRANS_SID_TAGS),
        &cstr_to_string(ptr),
    );
}

/// Returns the `session.trans_sid_hosts` string (default: `""`). Valid until
/// the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_trans_sid_hosts() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(TRANS_SID_HOSTS);
    if (*slot).is_none() {
        set_cstr(slot, "");
    }
    opt_ptr(core::ptr::addr_of!(TRANS_SID_HOSTS))
}

/// Sets the `session.trans_sid_hosts` string from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_trans_sid_hosts(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(TRANS_SID_HOSTS),
        &cstr_to_string(ptr),
    );
}

/// Returns the `session.upload_progress.enabled` flag (default: 1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_upload_progress_enabled() -> i64 {
    *core::ptr::addr_of!(UPLOAD_PROGRESS_ENABLED)
}

/// Sets the `session.upload_progress.enabled` flag. Config storage only.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_upload_progress_enabled(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(UPLOAD_PROGRESS_ENABLED), v);
}

/// Returns the `session.upload_progress.cleanup` flag (default: 1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_upload_progress_cleanup() -> i64 {
    *core::ptr::addr_of!(UPLOAD_PROGRESS_CLEANUP)
}

/// Sets the `session.upload_progress.cleanup` flag. Config storage only.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_upload_progress_cleanup(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(UPLOAD_PROGRESS_CLEANUP), v);
}

/// Returns the `session.upload_progress.prefix` string (default:
/// `"upload_progress_"`). Valid until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_upload_progress_prefix() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(UPLOAD_PROGRESS_PREFIX);
    if (*slot).is_none() {
        set_cstr(slot, "upload_progress_");
    }
    opt_ptr(core::ptr::addr_of!(UPLOAD_PROGRESS_PREFIX))
}

/// Sets the `session.upload_progress.prefix` string from a NUL-terminated C
/// string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_upload_progress_prefix(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(UPLOAD_PROGRESS_PREFIX),
        &cstr_to_string(ptr),
    );
}

/// Returns the `session.upload_progress.name` string (default:
/// `"PHP_SESSION_UPLOAD_PROGRESS"`). Valid until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_upload_progress_name() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(UPLOAD_PROGRESS_NAME);
    if (*slot).is_none() {
        set_cstr(slot, "PHP_SESSION_UPLOAD_PROGRESS");
    }
    opt_ptr(core::ptr::addr_of!(UPLOAD_PROGRESS_NAME))
}

/// Sets the `session.upload_progress.name` string from a NUL-terminated C
/// string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_upload_progress_name(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(UPLOAD_PROGRESS_NAME),
        &cstr_to_string(ptr),
    );
}

/// Returns the `session.upload_progress.freq` string (default: `"1%"`). Valid
/// until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_upload_progress_freq() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(UPLOAD_PROGRESS_FREQ);
    if (*slot).is_none() {
        set_cstr(slot, "1%");
    }
    opt_ptr(core::ptr::addr_of!(UPLOAD_PROGRESS_FREQ))
}

/// Sets the `session.upload_progress.freq` string from a NUL-terminated C
/// string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_upload_progress_freq(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(UPLOAD_PROGRESS_FREQ),
        &cstr_to_string(ptr),
    );
}

/// Returns the `session.upload_progress.min_freq` string (default: `"1"`).
/// Valid until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_upload_progress_min_freq() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(UPLOAD_PROGRESS_MIN_FREQ);
    if (*slot).is_none() {
        set_cstr(slot, "1");
    }
    opt_ptr(core::ptr::addr_of!(UPLOAD_PROGRESS_MIN_FREQ))
}

/// Sets the `session.upload_progress.min_freq` string from a NUL-terminated C
/// string.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_upload_progress_min_freq(ptr: *const c_char) {
    set_cstr(
        core::ptr::addr_of_mut!(UPLOAD_PROGRESS_MIN_FREQ),
        &cstr_to_string(ptr),
    );
}

/// Returns the per-request `session.auto_start` working copy (default: seeded
/// each request from `ELEPHC_SESSION_AUTO_START`). Read by the web prelude
/// bootstrap to decide whether to auto-call `session_start()`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_auto_start() -> i64 {
    *core::ptr::addr_of!(SESSION_AUTO_START)
}

/// Sets the per-request `session.auto_start` working copy. Setting it mid-request
/// does not retroactively start a session for the already-completed bootstrap;
/// the next request re-seeds this from the process-level env config.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_auto_start(v: i64) {
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_AUTO_START), v);
}

// ═══════════════════════════════════════════════════════════════════════════
// Per-request state reset
// ═══════════════════════════════════════════════════════════════════════════

/// Resets all session state to deployment defaults and releases any held file
/// lock. The worker calls this before draining a request body and the web prelude
/// repeats it before PHP execution. The shared `ELEPHC_SESSION_*` seed values
/// therefore drive both upload progress and the later `session_start()` call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_reset() {
    // Release any held session-file lock.
    super::file_io::release_lock();

    // Read immutable deployment configuration before resetting the per-request
    // working copies. Invalid values fall back to PHP's defaults.
    let configured_name = std::env::var("ELEPHC_SESSION_NAME")
        .ok()
        .filter(|value| {
            !value.is_empty()
                && value.parse::<f64>().is_err()
                && !value.bytes().any(|byte| {
                    matches!(
                        byte,
                        b'=' | b',' | b';' | b'.' | b'[' | b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c
                    )
                })
        })
        .and_then(|value| CString::new(value).ok());
    let configured_save_path = std::env::var("ELEPHC_SESSION_SAVE_PATH")
        .ok()
        .and_then(|value| CString::new(value).ok());
    let configured_serializer = std::env::var("ELEPHC_SESSION_SERIALIZE_HANDLER")
        .ok()
        .filter(|value| matches!(value.as_str(), "php" | "php_serialize" | "php_binary"))
        .and_then(|value| CString::new(value).ok());
    let configured_use_only_cookies = std::env::var("ELEPHC_SESSION_USE_ONLY_COOKIES")
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "off" | "false" | "no"
            )
        })
        .unwrap_or(true);
    let configured_upload_enabled = std::env::var("ELEPHC_SESSION_UPLOAD_PROGRESS_ENABLED")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "on" | "true"
            )
        })
        .unwrap_or(true);
    let configured_upload_cleanup = std::env::var("ELEPHC_SESSION_UPLOAD_PROGRESS_CLEANUP")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "on" | "true"
            )
        })
        .unwrap_or(true);

    // Reset all state to PHP defaults plus the deployment-level overrides.
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_NAME), configured_name);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_ID), None);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_STATUS), 1);
    core::ptr::write(
        core::ptr::addr_of_mut!(SESSION_SAVE_PATH),
        configured_save_path,
    );
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_CACHE_LIMITER), None);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_CACHE_EXPIRE), 180);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_LIFETIME), 0);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_PATH), None);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_DOMAIN), None);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_SECURE), false);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_PARTITIONED), false);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_HTTPONLY), false);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_SAMESITE), None);
    (*core::ptr::addr_of_mut!(SESSION_SNAPSHOT)).clear();
    (*core::ptr::addr_of_mut!(DATA_BUFFER)).clear();
    (*core::ptr::addr_of_mut!(PUBLISHED_BUFFERS)).clear();
    (*core::ptr::addr_of_mut!(STAGING_BUFFER)).clear();
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_FD), -1);
    core::ptr::write(core::ptr::addr_of_mut!(STRICT_MODE), 0);
    core::ptr::write(
        core::ptr::addr_of_mut!(SERIALIZE_HANDLER),
        configured_serializer,
    );
    core::ptr::write(core::ptr::addr_of_mut!(GC_PROBABILITY), 1);
    core::ptr::write(core::ptr::addr_of_mut!(GC_DIVISOR), 100);
    core::ptr::write(core::ptr::addr_of_mut!(GC_MAXLIFETIME), 1440);
    core::ptr::write(core::ptr::addr_of_mut!(SID_LENGTH), 32);
    core::ptr::write(core::ptr::addr_of_mut!(SID_BITS_PER_CHARACTER), 4);
    // v4 ini config layer: strings reset to None (getters re-init to defaults),
    // ints reset to their PHP defaults.
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_REFERER_CHECK), None);
    core::ptr::write(core::ptr::addr_of_mut!(USE_COOKIES), 1);
    core::ptr::write(
        core::ptr::addr_of_mut!(USE_ONLY_COOKIES),
        i64::from(configured_use_only_cookies),
    );
    core::ptr::write(core::ptr::addr_of_mut!(LAZY_WRITE), 1);
    core::ptr::write(core::ptr::addr_of_mut!(USE_TRANS_SID), 0);
    core::ptr::write(core::ptr::addr_of_mut!(TRANS_SID_TAGS), None);
    core::ptr::write(core::ptr::addr_of_mut!(TRANS_SID_HOSTS), None);
    core::ptr::write(
        core::ptr::addr_of_mut!(UPLOAD_PROGRESS_ENABLED),
        i64::from(configured_upload_enabled),
    );
    core::ptr::write(
        core::ptr::addr_of_mut!(UPLOAD_PROGRESS_CLEANUP),
        i64::from(configured_upload_cleanup),
    );
    core::ptr::write(core::ptr::addr_of_mut!(UPLOAD_PROGRESS_PREFIX), None);
    core::ptr::write(core::ptr::addr_of_mut!(UPLOAD_PROGRESS_NAME), None);
    core::ptr::write(core::ptr::addr_of_mut!(UPLOAD_PROGRESS_FREQ), None);
    core::ptr::write(core::ptr::addr_of_mut!(UPLOAD_PROGRESS_MIN_FREQ), None);
    // auto_start is seeded from the process-level env config, NOT a fixed
    // default, so a `session.auto_start=1` deployment auto-starts every request.
    core::ptr::write(
        core::ptr::addr_of_mut!(SESSION_AUTO_START),
        auto_start_config(),
    );
}

// ── Shared test infrastructure (used by every session submodule's tests) ──

/// Serializes tests that touch shared session process-statics across every
/// session submodule (`state`, `file_io`, `id`, `wire_format`). Without this,
/// parallel tests would race on the global session state (name, ID, status,
/// cookie params, fd, etc.) and produce spurious assertion failures.
#[cfg(test)]
pub(super) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquires the shared test serialization lock. Every state-touching test in
/// any session submodule calls this (via `state::test_lock`) at the top of the
/// test body. Uses `lock().unwrap_or_else` to recover from poison (a prior
/// test's panic should not cascade).
#[cfg(test)]
pub(super) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::test_lock as lock;
    use super::*;

    /// Verifies later publication and staging cannot overwrite a borrowed payload.
    #[test]
    fn staging_buffer_is_distinct_from_published_data() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let payload = b"hits|i:1;";
            let published = publish_bytes(payload) as *const u8;
            let staged = elephc_web_session_data_stage(payload.len() as i64) as *mut u8;
            core::ptr::copy_nonoverlapping(published, staged, payload.len());
            let _key = publish_bytes(b"hits");

            assert_eq!(std::slice::from_raw_parts(published, payload.len()), payload);
            assert_eq!(&*core::ptr::addr_of!(DATA_BUFFER), b"hits");
            assert_eq!(&*core::ptr::addr_of!(STAGING_BUFFER), payload);
        }
    }

    /// Verifies the session state defaults after a reset.
    #[test]
    fn session_defaults_after_reset() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let name = std::ffi::CStr::from_ptr(elephc_web_session_get_name());
            assert_eq!(name.to_str().unwrap(), "PHPSESSID");
            let id = std::ffi::CStr::from_ptr(elephc_web_session_get_id());
            assert_eq!(id.to_str().unwrap(), "");
            assert_eq!(elephc_web_session_get_status(), 1);
            let sp = std::ffi::CStr::from_ptr(elephc_web_session_get_save_path());
            assert_eq!(sp.to_str().unwrap(), "");
            let cl = std::ffi::CStr::from_ptr(elephc_web_session_get_cache_limiter());
            assert_eq!(cl.to_str().unwrap(), "nocache");
            assert_eq!(elephc_web_session_get_cache_expire(), 180);
            assert_eq!(elephc_web_session_get_cookie_lifetime(), 0);
            let path = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_path());
            assert_eq!(path.to_str().unwrap(), "/");
            let domain = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_domain());
            assert_eq!(domain.to_str().unwrap(), "");
            assert_eq!(elephc_web_session_get_cookie_secure(), 0);
            assert_eq!(elephc_web_session_get_cookie_partitioned(), 0);
            assert_eq!(elephc_web_session_get_cookie_httponly(), 0);
            let samesite = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_samesite());
            assert_eq!(samesite.to_str().unwrap(), "");
            assert_eq!(elephc_web_session_get_strict_mode(), 0);
            assert_eq!(elephc_web_session_get_use_cookies(), 1);
            assert_eq!(elephc_web_session_get_lazy_write(), 1);
            let sh = std::ffi::CStr::from_ptr(elephc_web_session_get_serialize_handler());
            assert_eq!(sh.to_str().unwrap(), "php");
            assert_eq!(elephc_web_session_get_gc_probability(), 1);
            assert_eq!(elephc_web_session_get_gc_divisor(), 100);
            assert_eq!(elephc_web_session_get_gc_maxlifetime(), 1440);
            assert_eq!(elephc_web_session_get_sid_length(), 32);
            assert_eq!(elephc_web_session_get_sid_bits_per_character(), 4);
        }
    }

    /// Verifies name get/set round-trips.
    #[test]
    fn session_name_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let n = std::ffi::CString::new("MySession").unwrap();
            elephc_web_session_set_name(n.as_ptr());
            let got = std::ffi::CStr::from_ptr(elephc_web_session_get_name());
            assert_eq!(got.to_str().unwrap(), "MySession");
        }
    }

    /// Verifies status get/set.
    #[test]
    fn session_status_set_get() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            elephc_web_session_set_status(2);
            assert_eq!(elephc_web_session_get_status(), 2);
            elephc_web_session_set_status(1);
            assert_eq!(elephc_web_session_get_status(), 1);
        }
    }

    /// Verifies cookie params set/get.
    #[test]
    fn cookie_params_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let path = std::ffi::CString::new("/app").unwrap();
            let domain = std::ffi::CString::new("example.com").unwrap();
            let samesite = std::ffi::CString::new("Strict").unwrap();
            elephc_web_session_set_cookie_params(
                3600,
                path.as_ptr(),
                domain.as_ptr(),
                1,
                1,
                0,
                samesite.as_ptr(),
            );
            assert_eq!(elephc_web_session_get_cookie_lifetime(), 3600);
            let p = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_path());
            assert_eq!(p.to_str().unwrap(), "/app");
            let d = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_domain());
            assert_eq!(d.to_str().unwrap(), "example.com");
            assert_eq!(elephc_web_session_get_cookie_secure(), 1);
            assert_eq!(elephc_web_session_get_cookie_partitioned(), 1);
            assert_eq!(elephc_web_session_get_cookie_httponly(), 0);
            let ss = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_samesite());
            assert_eq!(ss.to_str().unwrap(), "Strict");
        }
    }

    /// Verifies strict-mode and serialize-handler get/set round-trip
    /// (§2.2, §2.4).
    #[test]
    fn strict_mode_and_serialize_handler_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            assert_eq!(elephc_web_session_get_strict_mode(), 0);
            elephc_web_session_set_strict_mode(1);
            assert_eq!(elephc_web_session_get_strict_mode(), 1);

            let sh = std::ffi::CStr::from_ptr(elephc_web_session_get_serialize_handler());
            assert_eq!(sh.to_str().unwrap(), "php");
            let custom = std::ffi::CString::new("php_serialize").unwrap();
            elephc_web_session_set_serialize_handler(custom.as_ptr());
            let sh2 = std::ffi::CStr::from_ptr(elephc_web_session_get_serialize_handler());
            assert_eq!(sh2.to_str().unwrap(), "php_serialize");

            elephc_web_session_reset();
        }
    }

    /// Verifies GC probability/divisor/maxlifetime get/set round-trip (§2.6).
    #[test]
    fn gc_config_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            assert_eq!(elephc_web_session_get_gc_probability(), 1);
            assert_eq!(elephc_web_session_get_gc_divisor(), 100);
            assert_eq!(elephc_web_session_get_gc_maxlifetime(), 1440);
            elephc_web_session_set_gc_probability(0);
            elephc_web_session_set_gc_divisor(1000);
            elephc_web_session_set_gc_maxlifetime(7200);
            assert_eq!(elephc_web_session_get_gc_probability(), 0);
            assert_eq!(elephc_web_session_get_gc_divisor(), 1000);
            assert_eq!(elephc_web_session_get_gc_maxlifetime(), 7200);
            elephc_web_session_reset();
        }
    }

    /// Verifies sid_length/sid_bits_per_character setters accept the
    /// documented ranges and reject (leaving state unchanged) everything
    /// else (§2.7).
    #[test]
    fn sid_length_and_bits_validate_range() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            assert_eq!(elephc_web_session_get_sid_length(), 32);
            assert_eq!(elephc_web_session_get_sid_bits_per_character(), 4);

            // Out-of-range rejected, state unchanged.
            assert_eq!(elephc_web_session_set_sid_length(21), 0);
            assert_eq!(elephc_web_session_set_sid_length(257), 0);
            assert_eq!(elephc_web_session_get_sid_length(), 32);
            assert_eq!(elephc_web_session_set_sid_bits_per_character(3), 0);
            assert_eq!(elephc_web_session_set_sid_bits_per_character(7), 0);
            assert_eq!(elephc_web_session_get_sid_bits_per_character(), 4);

            // In-range boundary values accepted.
            assert_eq!(elephc_web_session_set_sid_length(22), 1);
            assert_eq!(elephc_web_session_get_sid_length(), 22);
            assert_eq!(elephc_web_session_set_sid_length(256), 1);
            assert_eq!(elephc_web_session_get_sid_length(), 256);
            assert_eq!(elephc_web_session_set_sid_bits_per_character(6), 1);
            assert_eq!(elephc_web_session_get_sid_bits_per_character(), 6);

            elephc_web_session_reset();
        }
    }

    /// Verifies the v4 ini config-layer statics (referer_check, use_only_cookies,
    /// use_trans_sid, trans_sid tags/hosts, and all upload_progress.* directives)
    /// carry their PHP defaults after a reset and round-trip through their
    /// getters/setters.
    #[test]
    fn ini_config_layer_defaults_and_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let rc = std::ffi::CStr::from_ptr(elephc_web_session_get_referer_check());
            assert_eq!(rc.to_str().unwrap(), "");
            assert_eq!(elephc_web_session_get_use_only_cookies(), 1);
            assert_eq!(elephc_web_session_get_use_trans_sid(), 0);
            let tags = std::ffi::CStr::from_ptr(elephc_web_session_get_trans_sid_tags());
            assert_eq!(tags.to_str().unwrap(), "a=href,area=href,frame=src,form=");
            let hosts = std::ffi::CStr::from_ptr(elephc_web_session_get_trans_sid_hosts());
            assert_eq!(hosts.to_str().unwrap(), "");
            assert_eq!(elephc_web_session_get_upload_progress_enabled(), 1);
            assert_eq!(elephc_web_session_get_upload_progress_cleanup(), 1);
            let pfx = std::ffi::CStr::from_ptr(elephc_web_session_get_upload_progress_prefix());
            assert_eq!(pfx.to_str().unwrap(), "upload_progress_");
            let upn = std::ffi::CStr::from_ptr(elephc_web_session_get_upload_progress_name());
            assert_eq!(upn.to_str().unwrap(), "PHP_SESSION_UPLOAD_PROGRESS");
            let freq = std::ffi::CStr::from_ptr(elephc_web_session_get_upload_progress_freq());
            assert_eq!(freq.to_str().unwrap(), "1%");
            let minf = std::ffi::CStr::from_ptr(elephc_web_session_get_upload_progress_min_freq());
            assert_eq!(minf.to_str().unwrap(), "1");

            // Round-trip a string and an int directive.
            let host = std::ffi::CString::new("example.com").unwrap();
            elephc_web_session_set_referer_check(host.as_ptr());
            let rc2 = std::ffi::CStr::from_ptr(elephc_web_session_get_referer_check());
            assert_eq!(rc2.to_str().unwrap(), "example.com");
            elephc_web_session_set_use_trans_sid(1);
            assert_eq!(elephc_web_session_get_use_trans_sid(), 1);

            // A reset clears the round-tripped values back to defaults.
            elephc_web_session_reset();
            let rc3 = std::ffi::CStr::from_ptr(elephc_web_session_get_referer_check());
            assert_eq!(rc3.to_str().unwrap(), "");
            assert_eq!(elephc_web_session_get_use_trans_sid(), 0);
        }
    }

    /// Verifies the `session.auto_start` per-request working copy round-trips
    /// through its getter/setter and is reset each request from the process-level
    /// env config (0 in the test environment, where `ELEPHC_SESSION_AUTO_START`
    /// is unset).
    #[test]
    fn auto_start_round_trip_and_reset() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            assert_eq!(elephc_web_session_get_auto_start(), 0);
            elephc_web_session_set_auto_start(1);
            assert_eq!(elephc_web_session_get_auto_start(), 1);
            elephc_web_session_reset();
            assert_eq!(elephc_web_session_get_auto_start(), 0);
        }
    }
}
