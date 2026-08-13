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

use crate::easy::{self, CurlSlist, CURL};

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
    /// The `struct curl_slist` this handle currently has set, keyed by the
    /// `CURLOPT_*` that owns it (`CURLOPT_HTTPHEADER`, `CURLOPT_QUOTE`, ...).
    ///
    /// libcurl does NOT copy a slist option: it stores the pointer and walks
    /// it during the transfer, so the list must outlive every
    /// `curl_easy_perform` on this handle. That is what this map is for —
    /// `elephc_curl_easy_setopt_slist` moves the previous list for an option
    /// out of here and frees it only AFTER libcurl has accepted the
    /// replacement, and `free_slists` releases whatever is left when the
    /// handle is reset or cleaned up.
    pub(crate) slists: HashMap<i32, *mut CurlSlist>,
}

// SAFETY: `*mut CURL` is not `Send` by default only because raw pointers make
// no promise about *what* they point to. What the `handles()` `Mutex` (and
// this impl) actually guarantee is narrower than "no two threads ever drive
// the same handle at once": the mutex only serializes access to the TABLE
// itself (insert/remove/lookup), not to an individual `EasyEntry`'s libcurl
// calls for the full duration of one operation. `crate::abi::
// elephc_curl_easy_perform` and `elephc_curl_easy_free` both deliberately
// DROP the table lock before calling into libcurl (`curl_easy_perform`/
// `curl_easy_cleanup`) — the write callback re-locks the table per chunk from
// the same thread during `perform`, which would deadlock a non-reentrant
// `Mutex` if the lock were held for the whole call. That means a `free(id)`
// running concurrently with an in-flight `perform(id)` on the SAME id from a
// DIFFERENT OS thread can run `curl_easy_cleanup` on the same `*mut CURL` a
// blocked `curl_easy_perform` is still using — a real use-after-free at the
// libcurl level that this mutex does NOT prevent.
//
// This is sound today only because the caller contract this crate assumes is
// upheld: elephc-compiled PHP programs are effectively single-threaded, so no
// two `elephc_curl_*` calls for the SAME id are ever in flight concurrently
// (mirrors the identical rationale in `elephc-pdo`'s connection/statement
// tables). Any future concurrent caller (e.g. a multi-threaded driver, or
// Task 9's multi interface if it ever fans work across OS threads) MUST
// itself guarantee no two threads call `elephc_curl_easy_perform`/`_free`/
// `_setopt_*`/`_take_body` for the SAME id concurrently — this table cannot
// enforce that without a per-handle lock, which is a deliberate redesign left
// out of scope here. See `crate::abi`'s module doc and the
// `elephc_curl_easy_perform`/`elephc_curl_easy_free` doc comments for the
// identical contract stated at the ABI boundary.
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
            slists: HashMap::new(),
        }
    }

    /// Frees every `struct curl_slist` this handle owns and forgets them.
    ///
    /// Called on `curl_reset()` and on handle teardown. The lists are freed
    /// AFTER `curl_easy_reset`/`curl_easy_cleanup` has dropped libcurl's own
    /// pointers to them at every call site, never before: libcurl walks a slist
    /// option during the transfer, so freeing one that is still set would leave
    /// a dangling pointer inside the easy handle.
    pub(crate) fn free_slists(&mut self) {
        for (_, list) in self.slists.drain() {
            // SAFETY: every pointer in this map came from `easy::slist_append`,
            // is owned solely by this entry, and is removed from the map as it
            // is freed, so no double free is reachable.
            unsafe { easy::slist_free_all(list) };
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
