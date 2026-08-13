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

use crate::callbacks::{CallbackSlot, SLOT_COUNT};
use crate::easy::{self, CurlSlist, CURL};
use crate::mime::{self, CurlMime, CurlMimePart};

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
    /// A borrowed-until-overwritten byte buffer for the string-shaped operations
    /// (`curl_getinfo()`'s string/list/array forms, and Wave D's
    /// `curl_escape`/`curl_unescape`), the same convention `taken_body` uses for
    /// the response body. Kept SEPARATE from `taken_body` so reading a header out
    /// of `curl_getinfo()` cannot invalidate a captured body the caller has not
    /// copied yet.
    pub(crate) scratch: Vec<u8>,
    /// The bridge id of the share (`crate::share`) this handle currently has attached
    /// via `CURLOPT_SHARE`, or `None` when unattached. Lets `elephc_curl_easy_set_share`
    /// detach from a PREVIOUS share when the option is set again to a different one, and
    /// lets `elephc_curl_easy_reset`/`elephc_curl_easy_free` remove this id from the
    /// share's own `attached` bookkeeping so it cannot grow unboundedly across many
    /// short-lived easy handles sharing one long-lived (in particular,
    /// `curl_share_init_persistent()`) share. See `crate::share`'s module doc for the
    /// full lifetime argument this field is part of.
    pub(crate) share_id: Option<i64>,
    /// The `curl_mime` structure currently ATTACHED to this handle via `CURLOPT_MIMEPOST`
    /// (Task 11's multipart/form-data uploads), or `None` when no array `CURLOPT_POSTFIELDS`
    /// has ever been applied. Owned by this entry the same way `slists` owns its lists:
    /// libcurl does not copy the structure a pointer-typed option carries, so it must
    /// outlive every `curl_easy_perform` on this handle, and is freed only on reset, free,
    /// or once a REPLACEMENT builder has been successfully attached (`crate::mime::post`).
    pub(crate) mime: Option<*mut CurlMime>,
    /// A `curl_mime` structure UNDER CONSTRUCTION (`crate::mime::new_pending`/`add_part`/
    /// `set_field`), not yet attached to the handle. Separate from `mime` so a
    /// `curl_setopt(..., CURLOPT_POSTFIELDS, $array)` call that fails partway through the
    /// PHP-level array walk (an unsupported value shape, a field libcurl itself refused)
    /// can discard the half-built structure (`crate::mime::abort`) without disturbing
    /// whatever mime is already attached from an earlier, successful call.
    pub(crate) pending_mime: Option<*mut CurlMime>,
    /// The part `set_field` currently writes to inside `pending_mime`, or `None` before the
    /// first `add_part` (or after `post`/`abort` clears the builder). Never independently
    /// freed: parts are owned by the `curl_mime` structure they belong to, and are released
    /// as a whole when that structure is freed.
    pub(crate) pending_part: Option<*mut CurlMimePart>,
    /// This entry's own bridge id. Duplicated out of the table key because the callback
    /// registrations in `crate::callbacks` carry it as libcurl `userdata`, and the
    /// trampolines look the handle back up with it — `apply_callback` needs it while it
    /// only holds a `&mut EasyEntry`.
    pub(crate) id: i64,
    /// The PHP callables installed through `curl_setopt()`'s callback options, indexed by
    /// `crate::callbacks::SLOT_*`. Borrowed, never owned: the descriptors are rooted in
    /// `CurlHandle` properties on the PHP side (see `crate::callbacks`).
    pub(crate) callbacks: [CallbackSlot; SLOT_COUNT],
    /// Whether `CURLOPT_WRITEFUNCTION` currently owns the body. With
    /// `return_transfer`, models php-src's single `handlers.write->method` enum:
    /// `write_user` = `PHP_CURL_USER`, `return_transfer` = `PHP_CURL_RETURN`, neither =
    /// `PHP_CURL_STDOUT`. The two are never both set — each setter clears the other.
    pub(crate) write_user: bool,
    /// Set when a PHP callback threw during the current transfer, so
    /// `elephc_curl_easy_perform` reports the failure without overwriting
    /// `curl_errno()` (php-src surfaces the exception with `curl_errno() === 0`).
    /// Cleared at the start of every `elephc_curl_easy_perform`.
    pub(crate) callback_threw: bool,
}

// SAFETY: `*mut CURL` is not `Send` by default only because raw pointers make
// no promise about *what* they point to. What the `handles()` `Mutex` (and
// this impl) actually guarantee is narrower than "no two threads ever drive
// the same handle at once": the mutex only serializes access to the TABLE
// itself (insert/remove/lookup), not to an individual `EasyEntry`'s libcurl
// calls for the full duration of one operation. `crate::abi::
// elephc_curl_easy_perform`, `elephc_curl_easy_pause` and
// `elephc_curl_easy_free` all deliberately DROP the table lock before calling
// into libcurl (`curl_easy_perform`/`curl_easy_pause`/`curl_easy_cleanup`) —
// the write callback re-locks the table per chunk from the same thread during
// `perform`, and `curl_easy_pause(CURLPAUSE_CONT)` flushes buffered data
// through that same callback, so either would deadlock a non-reentrant
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
    pub(crate) fn new(curl: *mut CURL, id: i64) -> Self {
        Self {
            curl,
            id,
            callbacks: [CallbackSlot::empty(); SLOT_COUNT],
            write_user: false,
            callback_threw: false,
            return_transfer: false,
            body: Vec::new(),
            taken_body: Vec::new(),
            error_buf: vec![0u8; easy::CURL_ERROR_SIZE],
            last_errno: 0,
            last_error: Vec::new(),
            slists: HashMap::new(),
            scratch: Vec::new(),
            share_id: None,
            mime: None,
            pending_mime: None,
            pending_part: None,
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

    /// Frees this handle's ATTACHED and PENDING `curl_mime` structures (if any) and forgets
    /// them, mirroring [`Self::free_slists`]. Called on `curl_reset()` and on handle
    /// teardown, AFTER `curl_easy_reset`/`curl_easy_cleanup` for the same reason
    /// `free_slists` is: libcurl may still hold a live pointer into the structure until the
    /// handle itself has been reset/torn down.
    pub(crate) fn free_mime(&mut self) {
        mime::free_all(self);
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
