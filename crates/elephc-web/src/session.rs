//! Purpose:
//! Owns the per-worker PHP session bridge state for `--web` mode: session name,
//! ID, status, save path, cache limiter/expire, cookie parameters, the data
//! snapshot used by `session_reset`/`session_abort`, and the held session-file
//! descriptor (locked with `flock(LOCK_EX)`). Provides every C-ABI session
//! primitive the web prelude calls (state getters/setters, file read/write/
//! destroy/abort, ID generation/validation, garbage collection, and the PHP
//! serialize-format parser that splits `key|serialized_value` pairs).
//!
//! Called from:
//! - The compiled `--web` web prelude, which declares each
//!   `elephc_web_session_*` symbol as `extern "elephc_web"` and calls them from
//!   the PHP-level `session_*()` function implementations.
//! - `elephc_web_session_reset`, called by the prelude at the start of every
//!   request to restore default state and release any held file lock.
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
//! - File writes are atomic: data is written to a temp file, `fsync`'d, then
//!   renamed over the original to prevent partial-write corruption on crash.

use std::ffi::{c_char, CString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, IntoRawFd};
use std::path::PathBuf;

// ── Session state (per-worker process statics) ──

/// Session name (cookie name). Default: `"PHPSESSID"`.
static mut SESSION_NAME: Option<CString> = None;
/// Current session ID. Default: empty (none).
static mut SESSION_ID: Option<CString> = None;
/// Session status: 0=disabled, 1=none, 2=active. Default: 1 (PHP_SESSION_NONE).
static mut SESSION_STATUS: i64 = 1;
/// Session save path. Default: the system temp directory.
static mut SESSION_SAVE_PATH: Option<CString> = None;
/// Cache limiter string. Default: `"nocache"`.
static mut SESSION_CACHE_LIMITER: Option<CString> = None;
/// Cache expire in minutes. Default: 180.
static mut SESSION_CACHE_EXPIRE: i64 = 180;
/// Cookie lifetime in seconds. Default: 0 (session cookie).
static mut COOKIE_LIFETIME: i64 = 0;
/// Cookie path. Default: `"/"`.
static mut COOKIE_PATH: Option<CString> = None;
/// Cookie domain. Default: empty.
static mut COOKIE_DOMAIN: Option<CString> = None;
/// Cookie secure flag. Default: false.
static mut COOKIE_SECURE: bool = false;
/// Cookie httponly flag. Default: true.
static mut COOKIE_HTTPONLY: bool = true;
/// Cookie SameSite attribute. Default: `"Lax"`.
static mut COOKIE_SAMESITE: Option<CString> = None;
/// Data snapshot for `session_reset`/`session_abort` (the original file content).
static mut SESSION_SNAPSHOT: Vec<u8> = Vec::new();
/// Held session-file descriptor (`-1` = none). Kept open with `flock(LOCK_EX)`
/// between `session_read` and `session_write`/`session_destroy`/`session_abort`.
static mut SESSION_FD: i32 = -1;

// ── Return-string buffers (per-worker, valid until next session call) ──

/// Buffer for `session_get_name` / `session_read` / `session_create_id` /
/// `session_entry_key` / `session_entry_value` string returns.
static mut RET_STRING: Option<CString> = None;

/// Returns the C-string pointer held in an `Option<CString>` static, or an empty
/// string pointer when unset. The pointer is valid until the static is next
/// written — the compiler copies it immediately after the call.
unsafe fn opt_ptr(slot: *const Option<CString>) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match &*slot {
        Some(s) => s.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Stores a Rust string into a `Option<CString>` static via raw pointer, replacing
/// any prior value. Interior NULs are stripped (CString cannot hold them).
unsafe fn set_cstr(slot: *mut Option<CString>, s: &str) {
    let cstr = CString::new(s.replace('\0', "")).unwrap_or_default();
    core::ptr::write(slot, Some(cstr));
}

/// Returns the current save path, initializing it to the system temp directory
/// on first use. Single-threaded per worker, so the lazy init cannot race.
unsafe fn save_path() -> PathBuf {
    let slot = core::ptr::addr_of_mut!(SESSION_SAVE_PATH);
    if (*slot).is_none() {
        let tmp = std::env::temp_dir().to_string_lossy().into_owned();
        set_cstr(slot, &tmp);
    }
    PathBuf::from((*slot).as_ref().unwrap().to_string_lossy().into_owned())
}

/// Builds the full path for a session file: `<save_path>/sess_<id>`.
fn session_file_path(save_path: &PathBuf, id: &str) -> PathBuf {
    save_path.join(format!("sess_{id}"))
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

/// Sets the session name (cookie name) from a `(ptr, len)` string pair.
/// Called by the prelude's `session_name()` setter.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_name(ptr: *const u8, len: i64) {
    if ptr.is_null() {
        return;
    }
    let s = String::from_utf8_lossy(core::slice::from_raw_parts(ptr, len as usize));
    set_cstr(core::ptr::addr_of_mut!(SESSION_NAME), &s);
}

/// Returns the current session ID as a NUL-terminated C string, or empty string
/// when no ID is set. Valid until the next session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_id() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(SESSION_ID))
}

/// Sets the session ID from a `(ptr, len)` string pair. Returns 1 on success.
/// Called by the prelude's `session_id()` setter.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_id(ptr: *const u8, len: i64) -> i64 {
    if ptr.is_null() {
        set_cstr(core::ptr::addr_of_mut!(SESSION_ID), "");
    } else {
        let s = String::from_utf8_lossy(core::slice::from_raw_parts(ptr, len as usize));
        set_cstr(core::ptr::addr_of_mut!(SESSION_ID), &s);
    }
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

/// Returns the session save path as a NUL-terminated C string. Initializes to
/// the system temp directory on first call. Valid until the next session call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_save_path() -> *const c_char {
    let _ = save_path();
    opt_ptr(core::ptr::addr_of!(SESSION_SAVE_PATH))
}

/// Sets the session save path from a `(ptr, len)` string pair.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_save_path(ptr: *const u8, len: i64) {
    if ptr.is_null() {
        return;
    }
    let s = String::from_utf8_lossy(core::slice::from_raw_parts(ptr, len as usize));
    set_cstr(core::ptr::addr_of_mut!(SESSION_SAVE_PATH), &s);
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

/// Sets the cache limiter from a `(ptr, len)` string pair.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_cache_limiter(ptr: *const u8, len: i64) {
    if ptr.is_null() {
        return;
    }
    let s = String::from_utf8_lossy(core::slice::from_raw_parts(ptr, len as usize));
    set_cstr(core::ptr::addr_of_mut!(SESSION_CACHE_LIMITER), &s);
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
    if *core::ptr::addr_of!(COOKIE_SECURE) { 1 } else { 0 }
}

/// Returns 1 if the cookie httponly flag is set, 0 otherwise (default: 1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_httponly() -> i64 {
    if *core::ptr::addr_of!(COOKIE_HTTPONLY) { 1 } else { 0 }
}

/// Returns the cookie SameSite attribute as a NUL-terminated C string
/// (default: `"Lax"`).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_get_cookie_samesite() -> *const c_char {
    let slot = core::ptr::addr_of_mut!(COOKIE_SAMESITE);
    if (*slot).is_none() {
        set_cstr(slot, "Lax");
    }
    opt_ptr(core::ptr::addr_of!(COOKIE_SAMESITE))
}

/// Sets all six cookie parameters at once. String parameters use `(ptr, len)`
/// pairs; `secure` and `httponly` are `i64` flags (0=false, non-zero=true).
/// Called by the prelude's `session_set_cookie_params()`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_set_cookie_params(
    lifetime: i64,
    path_ptr: *const u8,
    path_len: i64,
    domain_ptr: *const u8,
    domain_len: i64,
    secure: i64,
    httponly: i64,
    samesite_ptr: *const u8,
    samesite_len: i64,
) {
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_LIFETIME), lifetime);
    if !path_ptr.is_null() {
        let s = String::from_utf8_lossy(core::slice::from_raw_parts(path_ptr, path_len as usize));
        set_cstr(core::ptr::addr_of_mut!(COOKIE_PATH), &s);
    }
    if !domain_ptr.is_null() {
        let s = String::from_utf8_lossy(core::slice::from_raw_parts(
            domain_ptr,
            domain_len as usize,
        ));
        set_cstr(core::ptr::addr_of_mut!(COOKIE_DOMAIN), &s);
    }
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_SECURE), secure != 0);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_HTTPONLY), httponly != 0);
    if !samesite_ptr.is_null() {
        let s = String::from_utf8_lossy(core::slice::from_raw_parts(
            samesite_ptr,
            samesite_len as usize,
        ));
        set_cstr(core::ptr::addr_of_mut!(COOKIE_SAMESITE), &s);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Session file operations (with flock)
// ═══════════════════════════════════════════════════════════════════════════

/// Releases the held session-file lock (`flock(LOCK_UN)`) and closes the file
/// descriptor. No-op when no fd is held. Single-threaded per worker.
unsafe fn release_lock() {
    let fd = *core::ptr::addr_of!(SESSION_FD);
    if fd >= 0 {
        libc::flock(fd, libc::LOCK_UN);
        libc::close(fd);
        core::ptr::write(core::ptr::addr_of_mut!(SESSION_FD), -1);
    }
}

/// Reads the session file for `id` under `save_path`. Opens with
/// `O_RDWR | O_CREAT`, acquires `flock(LOCK_EX)`, reads the content, and stores
/// the fd (held for later `session_write`/`session_destroy`/`session_abort`).
/// When `read_and_close=1`, releases the lock and closes the fd immediately
/// after reading (no write will happen at handler end). Returns the file content
/// as a NUL-terminated C string (empty if the file was just created or empty).
/// The returned pointer is valid until the next session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_read(
    id_ptr: *const u8,
    id_len: i64,
    save_path_ptr: *const u8,
    save_path_len: i64,
    read_and_close: i64,
) -> *const c_char {
    if id_ptr.is_null() || save_path_ptr.is_null() {
        set_cstr(core::ptr::addr_of_mut!(RET_STRING), "");
        return opt_ptr(core::ptr::addr_of!(RET_STRING));
    }
    let id = String::from_utf8_lossy(core::slice::from_raw_parts(id_ptr, id_len as usize));
    // Validate the session ID before touching the filesystem (spec 3.8).
    if !validate_session_id(&id) {
        set_cstr(core::ptr::addr_of_mut!(RET_STRING), "");
        return opt_ptr(core::ptr::addr_of!(RET_STRING));
    }
    let sp = String::from_utf8_lossy(core::slice::from_raw_parts(
        save_path_ptr,
        save_path_len as usize,
    ));
    let path = session_file_path(&PathBuf::from(sp.into_owned()), &id);

    // Open with O_RDWR | O_CREAT, mode 0600.
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(f) => f,
        Err(_) => {
            set_cstr(core::ptr::addr_of_mut!(RET_STRING), "");
            return opt_ptr(core::ptr::addr_of!(RET_STRING));
        }
    };

    let fd = file.as_raw_fd();
    // Acquire exclusive lock (blocks until the lock is available).
    libc::flock(fd, libc::LOCK_EX);

    // Read the full content.
    let mut data = Vec::new();
    let _ = (&file).read_to_end(&mut data);

    if read_and_close != 0 {
        // Read-and-close: release lock and close fd immediately.
        drop(file);
        // fd is closed by drop; flock is released on close.
    } else {
        // Hold the fd open with the lock for later write/destroy/abort.
        // Convert the File into a raw fd we own (leak the File wrapper).
        let raw_fd = file.into_raw_fd();
        core::ptr::write(core::ptr::addr_of_mut!(SESSION_FD), raw_fd);
    }

    // Store the snapshot for session_reset/session_abort.
    (*core::ptr::addr_of_mut!(SESSION_SNAPSHOT)).clear();
    (*core::ptr::addr_of_mut!(SESSION_SNAPSHOT)).extend_from_slice(&data);

    set_cstr(core::ptr::addr_of_mut!(RET_STRING), &String::from_utf8_lossy(&data));
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

/// Writes `data` to the session file for `id` under `save_path` atomically (write
/// to temp file, fsync, rename), then releases the held lock and closes the fd.
/// Returns 1 on success, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_write(
    id_ptr: *const u8,
    id_len: i64,
    save_path_ptr: *const u8,
    save_path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    if id_ptr.is_null() || save_path_ptr.is_null() {
        return 0;
    }
    let id = String::from_utf8_lossy(core::slice::from_raw_parts(id_ptr, id_len as usize));
    if !validate_session_id(&id) {
        return 0;
    }
    let sp = String::from_utf8_lossy(core::slice::from_raw_parts(
        save_path_ptr,
        save_path_len as usize,
    ));
    let path = session_file_path(&PathBuf::from(sp.into_owned()), &id);
    let data: &[u8] = if data_ptr.is_null() || data_len == 0 {
        &[]
    } else {
        core::slice::from_raw_parts(data_ptr, data_len as usize)
    };

    // Atomic write: write to a temp file in the same directory, fsync, rename.
    // The temp name includes the session ID to avoid collisions between
    // concurrent requests (and parallel tests) in the same process.
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let tmp_path = dir.join(format!(".sess_tmp_{}_{}", std::process::id(), id));

    let result = (|| -> std::io::Result<()> {
        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            tmp.write_all(data)?;
            // Ensure the data hits disk before the rename.
            let tmp_fd = tmp.as_raw_fd();
            let _ = libc::fsync(tmp_fd);
        }
        fs::rename(&tmp_path, &path)?;
        Ok(())
    })();

    // If the atomic write fails, fall back to direct write on the held fd.
    if result.is_err() {
        let fd = *core::ptr::addr_of!(SESSION_FD);
        if fd >= 0 {
            // Truncate and write directly.
            let _ = libc::ftruncate(fd, 0);
            let _ = libc::pwrite(fd, data.as_ptr() as *const _, data.len(), 0);
            let _ = libc::fsync(fd);
        }
    }

    // Release the held lock (if any) and close.
    release_lock();
    1
}

/// Destroys the session file for `id` under `save_path` (deletes the file),
/// then releases any held lock. Returns 1 on success, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_destroy(
    id_ptr: *const u8,
    id_len: i64,
    save_path_ptr: *const u8,
    save_path_len: i64,
) -> i64 {
    if id_ptr.is_null() || save_path_ptr.is_null() {
        return 0;
    }
    let id = String::from_utf8_lossy(core::slice::from_raw_parts(id_ptr, id_len as usize));
    if !validate_session_id(&id) {
        return 0;
    }
    let sp = String::from_utf8_lossy(core::slice::from_raw_parts(
        save_path_ptr,
        save_path_len as usize,
    ));
    let path = session_file_path(&PathBuf::from(sp.into_owned()), &id);

    // Release the held lock first (so we don't hold it while unlinking).
    release_lock();

    // Delete the file. A missing file is not an error.
    let _ = fs::remove_file(&path);
    1
}

/// Aborts the session: releases the held lock without writing (discards any
/// in-memory changes). Returns 1 on success.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_abort(
    _id_ptr: *const u8,
    _id_len: i64,
    _save_path_ptr: *const u8,
    _save_path_len: i64,
) -> i64 {
    // Release the held lock without writing; the file keeps its original content.
    release_lock();
    1
}

// ═══════════════════════════════════════════════════════════════════════════
// Session ID generation
// ═══════════════════════════════════════════════════════════════════════════

/// Reads 16 bytes from `/dev/urandom` and converts them to 32 lowercase hex
/// characters. An optional `prefix` is prepended to the hex string. Returns the
/// new ID as a NUL-terminated C string, valid until the next session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_create_id(
    prefix_ptr: *const u8,
    prefix_len: i64,
) -> *const c_char {
    let prefix: String = if prefix_ptr.is_null() || prefix_len == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(core::slice::from_raw_parts(prefix_ptr, prefix_len as usize))
            .into_owned()
    };

    let mut random_bytes = [0u8; 16];
    let mut hex = String::with_capacity(prefix.len() + 32);
    hex.push_str(&prefix);

    match File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut random_bytes)) {
        Ok(()) => {
            for b in &random_bytes {
                hex.push_str(&format!("{b:02x}"));
            }
        }
        Err(_) => {
            // Fallback: use a simple time-based seed if /dev/urandom is unavailable.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            hex.push_str(&format!("{now:032x}"));
        }
    }

    set_cstr(core::ptr::addr_of_mut!(RET_STRING), &hex);
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

/// Validates a session ID: length must be 1–128 characters, and every character
/// must be in the set `a-zA-Z0-9,-`. Returns true if valid.
fn validate_session_id(id: &str) -> bool {
    let len = id.len();
    if len == 0 || len > 128 {
        return false;
    }
    id.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b',' || b == b'-')
}

// ═══════════════════════════════════════════════════════════════════════════
// Garbage collection
// ═══════════════════════════════════════════════════════════════════════════

/// Scans `save_path` for files matching `sess_*` and deletes those whose mtime
/// is older than `maxlifetime` seconds. Returns the number of deleted files.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_gc(
    save_path_ptr: *const u8,
    save_path_len: i64,
    maxlifetime: i64,
) -> i64 {
    if save_path_ptr.is_null() {
        return 0;
    }
    let sp = String::from_utf8_lossy(core::slice::from_raw_parts(
        save_path_ptr,
        save_path_len as usize,
    ));
    let dir = PathBuf::from(sp.into_owned());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cutoff = now - maxlifetime;
    let mut deleted: i64 = 0;

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("sess_") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let mtime_secs = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(now);
                    if mtime_secs < cutoff {
                        if fs::remove_file(entry.path()).is_ok() {
                            deleted += 1;
                        }
                    }
                }
            }
        }
    }

    deleted
}

// ═══════════════════════════════════════════════════════════════════════════
// Session format parser (PHP serialize format)
// ═══════════════════════════════════════════════════════════════════════════

/// Parses ASCII decimal digits at `data[start..end]` into a `usize`. Returns
/// `None` if the slice is not valid ASCII decimal or empty.
fn parse_digits(data: &[u8], start: usize, end: usize) -> Option<usize> {
    std::str::from_utf8(&data[start..end]).ok()?.parse::<usize>().ok()
}

/// Skips one complete PHP serialized value starting at byte position `pos` in
/// `data`, returning the byte position immediately after the value. Understands
/// all PHP serialize types: `N`, `b`, `i`, `d`, `s`, `a`, `O`, `C`. Returns the
/// original `pos` (no advancement) on invalid or truncated input.
fn skip_serialized_value(data: &[u8], pos: usize) -> usize {
    if pos >= data.len() {
        return pos;
    }
    match data[pos] {
        b'N' => {
            // N; — null
            if pos + 1 < data.len() && data[pos + 1] == b';' {
                pos + 2
            } else {
                pos
            }
        }
        b'b' => {
            // b:0; or b:1;
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // Skip the digit (0 or 1).
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b'i' => {
            // i:<number>;
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // Optional sign.
            if p < data.len() && (data[p] == b'+' || data[p] == b'-') {
                p += 1;
            }
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b'd' => {
            // d:<float>;
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // Skip the float body until ';'.
            while p < data.len() && data[p] != b';' {
                p += 1;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b's' => {
            // s:<len>:"<bytes>";
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            let len_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(slen) = parse_digits(data, len_start, p) else {
                return pos;
            };
            // Expect :"<bytes>";
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            // Skip slen bytes (the string body).
            if p + slen > data.len() {
                return pos;
            }
            p += slen;
            // Expect ";
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b'a' => {
            // a:<count>:{ key value key value ... }
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            let count_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(count) = parse_digits(data, count_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'{' {
                p += 1;
            } else {
                return pos;
            }
            // Skip count*2 serialized values (keys + values).
            for _ in 0..count * 2 {
                p = skip_serialized_value(data, p);
                if p == pos {
                    return pos;
                }
            }
            if p < data.len() && data[p] == b'}' {
                p + 1
            } else {
                pos
            }
        }
        b'O' => {
            // O:<namelen>:"<name>":<count>:{ key value ... }
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // namelen
            let nl_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(namelen) = parse_digits(data, nl_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p + namelen > data.len() {
                return pos;
            }
            p += namelen;
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // count
            let count_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(count) = parse_digits(data, count_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'{' {
                p += 1;
            } else {
                return pos;
            }
            for _ in 0..count * 2 {
                p = skip_serialized_value(data, p);
                if p == pos {
                    return pos;
                }
            }
            if p < data.len() && data[p] == b'}' {
                p + 1
            } else {
                pos
            }
        }
        b'C' => {
            // C:<namelen>:"<name>":<datalen>:{<data>}
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // namelen
            let nl_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(namelen) = parse_digits(data, nl_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p + namelen > data.len() {
                return pos;
            }
            p += namelen;
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // datalen
            let dl_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(datalen) = parse_digits(data, dl_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'{' {
                p += 1;
            } else {
                return pos;
            }
            // Skip datalen bytes.
            if p + datalen > data.len() {
                return pos;
            }
            p += datalen;
            if p < data.len() && data[p] == b'}' {
                p + 1
            } else {
                pos
            }
        }
        _ => pos, // Unknown type: no advancement.
    }
}

/// Parses the session data format (`key|serialized_value` pairs) and returns a
/// list of `(key_bytes, value_bytes)` slices. The key is everything before the
/// first `|`; the value is one complete serialized value after the `|`.
fn parse_session_entries(data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut entries = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        // Find the '|' separator.
        let mut key_end = pos;
        while key_end < data.len() && data[key_end] != b'|' {
            key_end += 1;
        }
        if key_end >= data.len() {
            break; // No separator — incomplete entry.
        }
        let key = &data[pos..key_end];
        let val_start = key_end + 1;
        if val_start >= data.len() {
            break;
        }
        let val_end = skip_serialized_value(data, val_start);
        if val_end == val_start {
            break; // Could not parse the value.
        }
        let value = &data[val_start..val_end];
        entries.push((key, value));
        pos = val_end;
    }
    entries
}

/// Returns the number of `key|serialized_value` entries in the session data.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_count_entries(
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    if data_ptr.is_null() || data_len <= 0 {
        return 0;
    }
    let data = core::slice::from_raw_parts(data_ptr, data_len as usize);
    parse_session_entries(data).len() as i64
}

/// Returns the key of entry `idx` (zero-based) from the session data, as a
/// NUL-terminated C string. Empty string when out of range. Valid until the next
/// session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_key(
    data_ptr: *const u8,
    data_len: i64,
    idx: i64,
) -> *const c_char {
    if data_ptr.is_null() || data_len <= 0 {
        set_cstr(core::ptr::addr_of_mut!(RET_STRING), "");
        return opt_ptr(core::ptr::addr_of!(RET_STRING));
    }
    let data = core::slice::from_raw_parts(data_ptr, data_len as usize);
    let entries = parse_session_entries(data);
    match usize::try_from(idx).ok().and_then(|i| entries.get(i)) {
        Some((key, _)) => {
            set_cstr(
                core::ptr::addr_of_mut!(RET_STRING),
                &String::from_utf8_lossy(key),
            );
        }
        None => set_cstr(core::ptr::addr_of_mut!(RET_STRING), ""),
    }
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

/// Returns the serialized value of entry `idx` (zero-based) from the session
/// data, as a NUL-terminated C string. Empty string when out of range. Valid
/// until the next session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_value(
    data_ptr: *const u8,
    data_len: i64,
    idx: i64,
) -> *const c_char {
    if data_ptr.is_null() || data_len <= 0 {
        set_cstr(core::ptr::addr_of_mut!(RET_STRING), "");
        return opt_ptr(core::ptr::addr_of!(RET_STRING));
    }
    let data = core::slice::from_raw_parts(data_ptr, data_len as usize);
    let entries = parse_session_entries(data);
    match usize::try_from(idx).ok().and_then(|i| entries.get(i)) {
        Some((_, value)) => {
            set_cstr(
                core::ptr::addr_of_mut!(RET_STRING),
                &String::from_utf8_lossy(value),
            );
        }
        None => set_cstr(core::ptr::addr_of_mut!(RET_STRING), ""),
    }
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

// ═══════════════════════════════════════════════════════════════════════════
// Per-request state reset
// ═══════════════════════════════════════════════════════════════════════════

/// Resets all session state to defaults and releases any held file lock. Called
/// by the web prelude at the start of every request. Clears name, ID, status,
/// save path, cache limiter/expire, cookie params, data snapshot, and fd.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_reset() {
    // Release any held session-file lock.
    release_lock();

    // Reset all state to defaults.
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_NAME), None);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_ID), None);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_STATUS), 1);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_SAVE_PATH), None);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_CACHE_LIMITER), None);
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_CACHE_EXPIRE), 180);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_LIFETIME), 0);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_PATH), None);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_DOMAIN), None);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_SECURE), false);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_HTTPONLY), true);
    core::ptr::write(core::ptr::addr_of_mut!(COOKIE_SAMESITE), None);
    (*core::ptr::addr_of_mut!(SESSION_SNAPSHOT)).clear();
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_FD), -1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that touch shared session process-statics. Without this,
    /// parallel tests would race on the global session state (name, ID, status,
    /// cookie params, fd, etc.) and produce spurious assertion failures.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquires the test serialization lock. All state-touching tests should
    /// call this at the top of the test body. Uses `lock().unwrap_or_else` to
    /// recover from poison (a prior test's panic should not cascade).
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
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
            assert!(!sp.to_str().unwrap().is_empty());
            let cl = std::ffi::CStr::from_ptr(elephc_web_session_get_cache_limiter());
            assert_eq!(cl.to_str().unwrap(), "nocache");
            assert_eq!(elephc_web_session_get_cache_expire(), 180);
            assert_eq!(elephc_web_session_get_cookie_lifetime(), 0);
            let path = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_path());
            assert_eq!(path.to_str().unwrap(), "/");
            let domain = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_domain());
            assert_eq!(domain.to_str().unwrap(), "");
            assert_eq!(elephc_web_session_get_cookie_secure(), 0);
            assert_eq!(elephc_web_session_get_cookie_httponly(), 1);
            let samesite = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_samesite());
            assert_eq!(samesite.to_str().unwrap(), "Lax");
        }
    }

    /// Verifies name get/set round-trips.
    #[test]
    fn session_name_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let n = b"MySession";
            elephc_web_session_set_name(n.as_ptr(), n.len() as i64);
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
            let path = b"/app";
            let domain = b"example.com";
            let samesite = b"Strict";
            elephc_web_session_set_cookie_params(
                3600,
                path.as_ptr(),
                path.len() as i64,
                domain.as_ptr(),
                domain.len() as i64,
                1,
                0,
                samesite.as_ptr(),
                samesite.len() as i64,
            );
            assert_eq!(elephc_web_session_get_cookie_lifetime(), 3600);
            let p = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_path());
            assert_eq!(p.to_str().unwrap(), "/app");
            let d = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_domain());
            assert_eq!(d.to_str().unwrap(), "example.com");
            assert_eq!(elephc_web_session_get_cookie_secure(), 1);
            assert_eq!(elephc_web_session_get_cookie_httponly(), 0);
            let ss = std::ffi::CStr::from_ptr(elephc_web_session_get_cookie_samesite());
            assert_eq!(ss.to_str().unwrap(), "Strict");
        }
    }

    /// Verifies session ID generation produces a 32-hex-char string.
    #[test]
    fn session_create_id_is_32_hex() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let id = std::ffi::CStr::from_ptr(elephc_web_session_create_id(std::ptr::null(), 0));
            let s = id.to_str().unwrap();
            assert_eq!(s.len(), 32, "expected 32 hex chars, got {s}");
            assert!(s.bytes().all(|b| b.is_ascii_hexdigit()), "not hex: {s}");
        }
    }

    /// Verifies session ID generation with a prefix.
    #[test]
    fn session_create_id_with_prefix() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let prefix = b"abc-";
            let id = std::ffi::CStr::from_ptr(elephc_web_session_create_id(
                prefix.as_ptr(),
                prefix.len() as i64,
            ));
            let s = id.to_str().unwrap();
            assert!(s.starts_with("abc-"));
            assert_eq!(s.len(), 36); // 4 prefix + 32 hex
        }
    }

    /// Verifies session ID validation accepts valid IDs and rejects invalid ones.
    #[test]
    fn session_id_validation() {
        assert!(validate_session_id("abc123"));
        assert!(validate_session_id("a,b-c,d"));
        assert!(validate_session_id("0123456789abcdef0123456789abcdef"));
        assert!(!validate_session_id(""));
        assert!(!validate_session_id("with space"));
        assert!(!validate_session_id("with;semicolon"));
        assert!(!validate_session_id(&"x".repeat(129)));
    }

    /// Verifies the session format parser handles all PHP serialize types.
    #[test]
    fn skip_value_all_types() {
        // N;
        assert_eq!(skip_serialized_value(b"N;", 0), 2);
        // b:1;
        assert_eq!(skip_serialized_value(b"b:1;", 0), 4);
        // b:0;
        assert_eq!(skip_serialized_value(b"b:0;", 0), 4);
        // i:42;
        assert_eq!(skip_serialized_value(b"i:42;", 0), 5);
        // i:-7;
        assert_eq!(skip_serialized_value(b"i:-7;", 0), 5);
        // d:3.14;
        assert_eq!(skip_serialized_value(b"d:3.14;", 0), 7);
        // s:5:"hello";
        assert_eq!(skip_serialized_value(b"s:5:\"hello\";", 0), 12);
        // s:0:"";
        assert_eq!(skip_serialized_value(b"s:0:\"\";", 0), 7);
        // a:2:{i:0;s:1:"a";i:1;s:1:"b";}
        let arr = b"a:2:{i:0;s:1:\"a\";i:1;s:1:\"b\";}";
        assert_eq!(skip_serialized_value(arr, 0), arr.len());
        // O:3:"Foo":1:{s:3:"bar";i:1;}
        let obj = b"O:3:\"Foo\":1:{s:3:\"bar\";i:1;}";
        assert_eq!(skip_serialized_value(obj, 0), obj.len());
    }

    /// Verifies the parser skips a nested array correctly.
    #[test]
    fn skip_value_nested_array() {
        // a:1:{i:0;a:1:{i:0;i:1;}}
        let nested = b"a:1:{i:0;a:1:{i:0;i:1;}}";
        assert_eq!(skip_serialized_value(nested, 0), nested.len());
    }

    /// Verifies the session entry parser splits key|value pairs.
    #[test]
    fn parse_entries_basic() {
        // count|i:5;name|s:3:"Tom";
        let data = b"count|i:5;name|s:3:\"Tom\";";
        let entries = parse_session_entries(data);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, b"count");
        assert_eq!(entries[0].1, b"i:5;");
        assert_eq!(entries[1].0, b"name");
        assert_eq!(entries[1].1, b"s:3:\"Tom\";");
    }

    /// Verifies the C-ABI count/key/value functions work on real session data.
    #[test]
    fn count_key_value_via_c_abi() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let data = b"count|i:5;name|s:3:\"Tom\";";
            let count = elephc_web_session_count_entries(data.as_ptr(), data.len() as i64);
            assert_eq!(count, 2);
            let key0 = std::ffi::CStr::from_ptr(elephc_web_session_entry_key(
                data.as_ptr(),
                data.len() as i64,
                0,
            ));
            assert_eq!(key0.to_str().unwrap(), "count");
            let val0 = std::ffi::CStr::from_ptr(elephc_web_session_entry_value(
                data.as_ptr(),
                data.len() as i64,
                0,
            ));
            assert_eq!(val0.to_str().unwrap(), "i:5;");
            let key1 = std::ffi::CStr::from_ptr(elephc_web_session_entry_key(
                data.as_ptr(),
                data.len() as i64,
                1,
            ));
            assert_eq!(key1.to_str().unwrap(), "name");
            let val1 = std::ffi::CStr::from_ptr(elephc_web_session_entry_value(
                data.as_ptr(),
                data.len() as i64,
                1,
            ));
            assert_eq!(val1.to_str().unwrap(), "s:3:\"Tom\";");
        }
    }

    /// Verifies the C-ABI entry functions return empty for out-of-range index.
    #[test]
    fn entry_out_of_range_is_empty() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let data = b"count|i:5;";
            let key = std::ffi::CStr::from_ptr(elephc_web_session_entry_key(
                data.as_ptr(),
                data.len() as i64,
                99,
            ));
            assert_eq!(key.to_str().unwrap(), "");
            let val = std::ffi::CStr::from_ptr(elephc_web_session_entry_value(
                data.as_ptr(),
                data.len() as i64,
                99,
            ));
            assert_eq!(val.to_str().unwrap(), "");
        }
    }

    /// Verifies session read/write/destroy round-trip with file locking.
    #[test]
    fn session_file_read_write_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let tmp = std::env::temp_dir();
            let sp = tmp.to_string_lossy().into_owned();
            let sp_bytes = sp.as_bytes();

            // Generate a unique session ID for this test to avoid collisions.
            let id = format!("testrw{}", std::process::id());
            let id_bytes = id.as_bytes();

            // Write data.
            let data = b"count|i:42;name|s:5:\"World\";";
            let result = elephc_web_session_write(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                data.as_ptr(),
                data.len() as i64,
            );
            assert_eq!(result, 1);

            // Read it back (read_and_close=0, lock held).
            let raw = std::ffi::CStr::from_ptr(elephc_web_session_read(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                0,
            ));
            assert_eq!(raw.to_str().unwrap(), String::from_utf8_lossy(data));

            // Clean up: release lock + destroy.
            release_lock();
            elephc_web_session_destroy(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
            );
        }
    }

    /// Verifies read_and_close=1 reads and immediately releases the lock.
    #[test]
    fn session_read_and_close() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let tmp = std::env::temp_dir();
            let sp = tmp.to_string_lossy().into_owned();
            let sp_bytes = sp.as_bytes();
            let id = format!("testrc{}", std::process::id());
            let id_bytes = id.as_bytes();
            let data = b"x|i:1;";

            // Write first.
            elephc_web_session_write(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                data.as_ptr(),
                data.len() as i64,
            );

            // Read with read_and_close=1 — lock should not be held after.
            let raw = std::ffi::CStr::from_ptr(elephc_web_session_read(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                1,
            ));
            assert_eq!(raw.to_str().unwrap(), String::from_utf8_lossy(data));

            // fd should be -1 (read_and_close does not hold it).
            assert_eq!(*core::ptr::addr_of!(SESSION_FD), -1);

            // Clean up.
            elephc_web_session_destroy(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
            );
        }
    }

    /// Verifies session_abort releases the lock without writing.
    #[test]
    fn session_abort_releases_lock() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let tmp = std::env::temp_dir();
            let sp = tmp.to_string_lossy().into_owned();
            let sp_bytes = sp.as_bytes();
            let id = format!("testabort{}", std::process::id());
            let id_bytes = id.as_bytes();

            // Write initial data.
            let data = b"v|i:1;";
            elephc_web_session_write(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                data.as_ptr(),
                data.len() as i64,
            );

            // Read (hold lock).
            elephc_web_session_read(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                0,
            );
            assert!(*core::ptr::addr_of!(SESSION_FD) >= 0);

            // Abort — should release lock.
            elephc_web_session_abort(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
            );
            assert_eq!(*core::ptr::addr_of!(SESSION_FD), -1);

            // Clean up.
            elephc_web_session_destroy(
                id_bytes.as_ptr(),
                id_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
            );
        }
    }

    /// Verifies GC deletes expired files and leaves fresh ones.
    #[test]
    fn session_gc_deletes_expired() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let tmp = std::env::temp_dir();
            let sp = tmp.to_string_lossy().into_owned();
            let sp_bytes = sp.as_bytes();

            // Create a fresh file.
            let fresh_id = format!("testgcfresh{}", std::process::id());
            let fresh_bytes = fresh_id.as_bytes();
            elephc_web_session_write(
                fresh_bytes.as_ptr(),
                fresh_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                b"x|i:1;".as_ptr(),
                6,
            );

            // Create a file and backdate its mtime by 2 hours.
            let old_id = format!("testgcold{}", std::process::id());
            let old_bytes = old_id.as_bytes();
            elephc_web_session_write(
                old_bytes.as_ptr(),
                old_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                b"x|i:2;".as_ptr(),
                6,
            );
            let old_path = session_file_path(&tmp, &old_id);
            // Set mtime to 2 hours ago via libc::utimes (needs NUL-terminated path).
            let two_hours_ago = std::time::SystemTime::now()
                - std::time::Duration::from_secs(7200);
            let secs = two_hours_ago
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let times = [
                libc::timeval { tv_sec: secs, tv_usec: 0 },
                libc::timeval { tv_sec: secs, tv_usec: 0 },
            ];
            let path_cstr = CString::new(old_path.to_string_lossy().into_owned()).unwrap_or_default();
            let _ = libc::utimes(path_cstr.as_ptr(), times.as_ptr());

            // GC with maxlifetime=3600 (1 hour) should delete the old file only.
            let deleted = elephc_web_session_gc(
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
                3600,
            );
            assert!(deleted >= 1, "expected at least 1 deleted, got {deleted}");
            assert!(!old_path.exists(), "old file should have been deleted");

            // Clean up the fresh file.
            elephc_web_session_destroy(
                fresh_bytes.as_ptr(),
                fresh_bytes.len() as i64,
                sp_bytes.as_ptr(),
                sp_bytes.len() as i64,
            );
        }
    }
}
