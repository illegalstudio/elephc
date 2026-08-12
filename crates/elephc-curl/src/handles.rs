//! Purpose:
//! The `i64`-keyed easy-handle table: `Mutex<HashMap<i64, EasyEntry>>`, its
//! monotonic id allocator, and the panic-firewall helpers every `#[no_mangle]`
//! entry point in `crate::abi` and the write callback in `crate::php_layer`
//! route through.
//!
//! Called from:
//! - `crate::abi` (every entry point locks the table by id).
//! - `crate::php_layer::write_callback` (looks up the handle mid-transfer).
//!
//! Key details:
//! - Ids are positive, monotonically increasing, and never reused, even after
//!   `elephc_curl_easy_free` — matching the brief's handle-table contract and
//!   avoiding any use-after-free-by-id class of bug if PHP-level code holds a
//!   stale id past a free.
//! - `ffi_guard`/`lock_recover` mirror the identical pattern in
//!   `elephc-pdo`/`elephc-image` (F-QUAL-02): every FFI entry point catches
//!   unwinds instead of letting a panic cross the `extern "C"` boundary
//!   (undefined behavior), and every lock acquisition recovers a poisoned
//!   mutex instead of re-panicking on every later call.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::easy::{self, CURL};

/// One tracked libcurl easy handle plus the PHP-layer state elephc keeps
/// around it: the RETURNTRANSFER capture flag/buffer, the most recently taken
/// body (kept alive so `elephc_curl_easy_take_body`'s returned pointer stays
/// valid until the next call that touches this id), and the last transfer's
/// error code/message (mirroring `curl_errno()`/`curl_error()`).
pub(crate) struct EasyEntry {
    /// The underlying libcurl handle. Freed by `curl_easy_cleanup` in
    /// `elephc_curl_easy_free`.
    pub(crate) curl: *mut CURL,
    /// Whether `CURLOPT_RETURNTRANSFER` is set: the write callback appends to
    /// `body` instead of streaming to stdout.
    pub(crate) return_transfer: bool,
    /// Bytes accumulated by the write callback during the current/last
    /// `curl_easy_perform`, when `return_transfer` is set. Reset to empty at
    /// the start of every `elephc_curl_easy_perform` call.
    pub(crate) body: Vec<u8>,
    /// The most recent `body` handed out by `elephc_curl_easy_take_body`,
    /// kept alive here so its pointer stays valid until the next call on this
    /// id (the same "valid until overwritten" convention `elephc-pdo`'s
    /// `store_bytes`/`elephc-image`'s `out_cell` use).
    pub(crate) taken_body: Vec<u8>,
    /// Fixed `CURL_ERROR_SIZE`-byte buffer handed to libcurl via
    /// `CURLOPT_ERRORBUFFER`. Never resized after `EasyEntry::new` allocates
    /// it: libcurl holds the raw pointer into this buffer for the handle's
    /// whole lifetime, and reallocating would invalidate it.
    pub(crate) error_buf: Vec<u8>,
    /// The `CURLcode` from the most recent `curl_easy_perform`, or `CURLE_OK`
    /// (`0`) if the handle has never performed a transfer — matching PHP's
    /// `curl_errno()`, which reports `0` before the first `curl_exec()`.
    pub(crate) last_errno: i32,
    /// The human-readable message libcurl wrote into `error_buf` during the
    /// most recent transfer, extracted up to the first NUL byte. Empty
    /// before the first transfer or after a transfer that set no message.
    pub(crate) last_error: Vec<u8>,
}

// SAFETY: `EasyEntry` is only ever reached through `handles()`'s `Mutex`,
// which serializes every access. `*mut CURL` is not `Send` by default only
// because raw pointers make no promise about *what* they point to; a libcurl
// easy handle itself has no thread affinity as long as it is never driven by
// two threads at once (libcurl's own documented contract), and the table
// mutex is exactly what prevents that. elephc-compiled programs are
// effectively single-threaded (mirroring the identical rationale in
// `elephc-pdo`'s connection/statement tables), so this is a simplicity
// trade-off, not contention management.
unsafe impl Send for EasyEntry {}

impl EasyEntry {
    /// Builds a fresh entry around an already-initialized `curl` handle, with
    /// its error buffer allocated and zeroed at the fixed size libcurl
    /// requires for `CURLOPT_ERRORBUFFER`.
    pub(crate) fn new(curl: *mut CURL) -> Self {
        Self {
            curl,
            return_transfer: false,
            body: Vec::new(),
            taken_body: Vec::new(),
            error_buf: vec![0u8; easy::CURL_ERROR_SIZE],
            last_errno: 0,
            last_error: Vec::new(),
        }
    }
}

/// Returns the process-wide easy-handle table guarded by a mutex, matching
/// the handle-table shape in the brief: `Mutex<HashMap<i64, EasyEntry>>`.
pub(crate) fn handles() -> &'static Mutex<HashMap<i64, EasyEntry>> {
    static HANDLES: OnceLock<Mutex<HashMap<i64, EasyEntry>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocates the next positive handle id. Monotonic and never reused, even
/// across `elephc_curl_easy_free` calls.
pub(crate) fn next_id() -> i64 {
    static NEXT_ID: AtomicI64 = AtomicI64::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

/// Locks the handle table, recovering the guard if a previously caught panic
/// poisoned it. Pairs with [`ffi_guard`]: once a panic escapes a body that
/// held this lock, the mutex stays poisoned forever, and a plain
/// `.lock().unwrap()` would then panic on every later curl call in the
/// process — unrelated handles included. The payload (a plain `HashMap`) is
/// still structurally valid after any panic this crate can trigger, so
/// reusing it lets the bridge keep serving other handles.
pub(crate) fn lock_recover<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs `body`, converting any caught panic into `fallback` instead of
/// letting it unwind across the `extern "C"` boundary (undefined behavior).
/// Every `#[no_mangle]` entry point in `crate::abi`, and the libcurl write
/// callback in `crate::php_layer`, route their whole body through this.
pub(crate) fn ffi_guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}
