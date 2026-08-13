//! Purpose:
//! PHP's curl SHARE interface end to end inside the bridge: the raw `curl_share_*`
//! libcurl bindings, the `i64`-keyed share-handle table (its own id space, mirroring
//! `crate::multi`'s), the `CURLSHOPT_*`/`CURL_LOCK_DATA_*` surface, the process-lifetime
//! "persistent share" registry behind PHP 8.5's `curl_share_init_persistent()`, and the
//! `elephc_curl_share_*` / `elephc_curl_easy_set_share` C ABI the compiled prelude calls.
//!
//! Called from:
//! - Compiled PHP programs, through the `__rt_curl_share_*` runtime helpers.
//! - `crate::tests`' gated real-libcurl tests.
//!
//! Key details:
//! - **THE LIFETIME HAZARD THIS MODULE EXISTS TO CLOSE.** libcurl requires a share object
//!   to outlive every easy handle that has `CURLOPT_SHARE` pointed at it — not merely
//!   "while a transfer using it is in flight" but for the WHOLE remaining lifetime of that
//!   easy handle, including its own `curl_easy_cleanup()`, which (libcurl 8.21.0,
//!   `lib/url.c`'s `Curl_close`) reads back through `data->share` to unlock/adjust the
//!   share's internal bookkeeping. `curl_share_cleanup()` on a share some live easy handle
//!   still points at is therefore a real use-after-free at the next thing that easy handle
//!   does, not merely a `CURLSHE_IN_USE` refusal (libcurl only refuses cleanup for
//!   CONCURRENT use across threads, which this single-threaded caller contract never
//!   produces — see `crate::handles::EasyEntry`'s `Send` impl for that same contract
//!   stated once).
//!
//!   **The design chosen here mirrors `crate::multi`'s own answer to the identical
//!   problem** (an attached easy handle outliving the multi handle it was added to): the
//!   BRIDGE, not a PHP-level object reference, is the source of truth. Every share entry
//!   tracks the ids of the easy handles currently attached to it
//!   ([`ShareEntry::attached`]), and [`elephc_curl_share_free`] walks that list and issues
//!   `curl_easy_setopt(easy, CURLOPT_SHARE, NULL)` for every one of them — clearing
//!   libcurl's own pointer back to this share — BEFORE `curl_share_cleanup()` ever runs.
//!   After that, every still-live easy handle that used to reference this share is exactly
//!   as usable as one that was never shared at all: `curl_exec()` on it still succeeds (it
//!   simply stops benefiting from the shared cache), and its own eventual
//!   `curl_easy_cleanup()` no longer touches freed memory. This is a strictly bridge-level
//!   guarantee — it holds regardless of what the PHP program does (`unset()` order,
//!   whether it ever calls `curl_share_close()`, which is a no-op exactly like
//!   `curl_close()`/`curl_multi_close()`), which is why no PHP-side strong reference from
//!   `CurlHandle` to `CurlShareHandle` is needed on top of it.
//!
//!   `elephc_curl_easy_set_share` keeps that list accurate across RE-attachment too: when
//!   an easy handle already attached to share A is pointed at share B, it is removed from
//!   A's `attached` list before being added to B's — otherwise A's eventual free would
//!   wrongly clear the easy handle's CURRENT (B) share. `elephc_curl_easy_reset` and
//!   `elephc_curl_easy_free` (`crate::abi`) do the same cleanup for the reset/free cases,
//!   which matters most for a long-lived (in particular persistent) share reused across
//!   many short-lived easy handles — see [`detach_easy`].
//! - **ONLY TWO `CURLSHOPT_*` VALUES ARE REAL PHP SURFACE.** `CURLSHOPT_SHARE` (1) and
//!   `CURLSHOPT_UNSHARE` (2) are the only `CURLSHOPT_*` constants PHP's `ext/curl` exposes
//!   at all (confirmed against `scripts/docs/curl_surface.json`, Task 1's frozen PHP
//!   8.2-8.5 extraction: `CURLSHOPT_LOCKFUNC`/`UNLOCKFUNC`/`USERDATA` are C-API-only
//!   locking hooks with no PHP constant naming them). php-src's own `curl_share_setopt()`
//!   switch has exactly two cases and a `default: zend_argument_value_error(...)`, so
//!   unlike `curl_multi_setopt()` there is no "real option this build cannot carry"
//!   bucket here at all — [`share_option_kind`] answers only [`SHARE_OPTION_LONG`] or
//!   [`SHARE_OPTION_INVALID`].
//! - **A REFUSED VALUE IS A PLAIN `false`, NEVER A FABRICATED WARNING.** When libcurl
//!   itself refuses the `CURL_LOCK_DATA_*` value (`CURLSHE_BAD_OPTION` for one it does not
//!   recognize, `CURLSHE_NOT_BUILT_IN` for a real one this libcurl build lacks), that is a
//!   genuine libcurl-level answer, not "this elephc build cannot carry a real PHP option"
//!   (locked decision 7's warning is for the LATTER). `curl_setopt()`'s own easy-option
//!   path draws exactly this distinction already: a carryable option libcurl refuses just
//!   answers `false`, with the real code retrievable through `curl_share_errno()`/
//!   `curl_share_strerror()`, never a warning invented for it.
//! - **`curl_share_init_persistent()` (PHP 8.5) reuses this SAME id space and table**,
//!   flagged [`ShareEntry::persistent`]. [`elephc_curl_share_free`] is a documented no-op
//!   for a persistent entry: the underlying share, once created, lives until the PROCESS
//!   exits (there is no elephc equivalent of PHP-FPM's worker-restart-scoped persistence
//!   to key a shorter lifetime off). Repeated calls with an equivalent option set — sorted
//!   and deduplicated, order- and duplicate-insensitive, php-src's own semantics — return
//!   the SAME share id via [`persistent_shares`].

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_long, c_void};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::easy::{self, CURL};
use crate::handles;

/// Opaque libcurl share-handle type, named after libcurl's own `CURLSH` typedef.
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum CURLSH {}

/// libcurl's `CURLSHcode`. `0` is `CURLSHE_OK`.
pub(crate) type CURLSHcode = c_int;

/// `CURLSHE_OK`: the share operation succeeded.
const CURLSHE_OK: CURLSHcode = 0;

extern "C" {
    /// Allocates a new share handle, or returns null on failure.
    fn curl_share_init() -> *mut CURLSH;

    /// Frees a share handle. Every still-attached easy handle is detached from it
    /// first by this crate (see [`elephc_curl_share_free`]).
    fn curl_share_cleanup(share: *mut CURLSH) -> CURLSHcode;

    /// The single variadic `curl_share_setopt` symbol, called only through
    /// [`elephc_curl_share_setopt`].
    fn curl_share_setopt(share: *mut CURLSH, option: c_int, ...) -> CURLSHcode;

    /// libcurl's `'static` human-readable text for a `CURLSHcode`.
    fn curl_share_strerror(code: CURLSHcode) -> *const c_char;
}

/// One tracked libcurl share handle.
struct ShareEntry {
    /// The underlying libcurl share handle, freed by `curl_share_cleanup` — unless
    /// [`ShareEntry::persistent`] is set, in which case it is never freed at all.
    share: *mut CURLSH,
    /// The bridge ids of the easy handles currently attached (`CURLOPT_SHARE` pointed at
    /// this entry), used only to detach them (`CURLOPT_SHARE` -> null) before
    /// `curl_share_cleanup` — see this module's header for the full lifetime argument.
    attached: Vec<i64>,
    /// The `CURLSHcode` from the most recent share operation, backing
    /// `curl_share_errno()`.
    last_errno: CURLSHcode,
    /// Set for a share minted by `curl_share_init_persistent()` (PHP 8.5): such a share is
    /// never freed by [`elephc_curl_share_free`], which is a documented no-op for it. See
    /// this module's header.
    persistent: bool,
}

// SAFETY: identical reasoning to `crate::multi::MultiEntry`'s `Send` impl — the table
// mutex serializes access to the TABLE, and the single-threaded caller contract this
// crate assumes throughout is what makes an individual `*mut CURLSH` safe to drive.
unsafe impl Send for ShareEntry {}

/// The process-wide share-handle table. A SEPARATE id space from the easy and multi
/// tables: a share id and an easy/multi id may all be `1`, and nothing ever passes one
/// where another is expected.
fn shares() -> &'static Mutex<HashMap<i64, ShareEntry>> {
    static SHARES: OnceLock<Mutex<HashMap<i64, ShareEntry>>> = OnceLock::new();
    SHARES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocates the next share id. Monotonic, positive, never reused.
fn next_share_id() -> i64 {
    static NEXT_ID: AtomicI64 = AtomicI64::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

/// Keys a persistent share by its SORTED, DEDUPLICATED `CURL_LOCK_DATA_*` option set onto
/// the share id created for it, so `curl_share_init_persistent()` called again with an
/// equivalent (order- and duplicate-insensitive) set returns the SAME share.
fn persistent_shares() -> &'static Mutex<HashMap<Vec<i64>, i64>> {
    static PERSISTENT: OnceLock<Mutex<HashMap<Vec<i64>, i64>>> = OnceLock::new();
    PERSISTENT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Looks the raw `*mut CURL` for an easy id up in the easy table, or `None` when the id
/// is unknown (already freed, or never existed) — the identical helper `crate::multi`
/// defines for the same reverse lookup.
fn easy_ptr(id: i64) -> Option<*mut CURL> {
    let guard = handles::lock_recover(handles::handles());
    guard.get(&id).map(|entry| entry.curl)
}

/// [`share_option_kind`]: not a `curl_share_setopt()` option at all -> php-src's own
/// `ValueError`. Everything except `CURLSHOPT_SHARE`/`CURLSHOPT_UNSHARE` lands here,
/// including the three locking-hook values PHP does not even expose as constants (see
/// this module's header).
pub(crate) const SHARE_OPTION_INVALID: i32 = 0;
/// [`share_option_kind`]: `curl_share_setopt(share, opt, long)` — `CURLSHOPT_SHARE` (1) or
/// `CURLSHOPT_UNSHARE` (2), both carrying a `CURL_LOCK_DATA_*` value as a `long`.
pub(crate) const SHARE_OPTION_LONG: i32 = 1;

/// Classifies a `curl_share_setopt()` option number. Returns [`SHARE_OPTION_LONG`] or
/// [`SHARE_OPTION_INVALID`] — see this module's header for why there is no third
/// "real option this build cannot carry" bucket the way `crate::multi::multi_option_kind`
/// needs one.
pub(crate) fn share_option_kind(opt: i64) -> i32 {
    match opt {
        1 | 2 => SHARE_OPTION_LONG,
        _ => SHARE_OPTION_INVALID,
    }
}

/// [`elephc_curl_share_setopt`]'s answer: the option was applied (`CURLSHE_OK`).
pub(crate) const SHARE_SETOPT_APPLIED: i32 = 1;
/// [`elephc_curl_share_setopt`]'s answer: a recognized option (`CURLSHOPT_SHARE`/
/// `UNSHARE`) that libcurl itself refused (a `CURL_LOCK_DATA_*` value it does not
/// recognize or was not built with) -> PHP `false`, no warning (see this module's
/// header). The real `CURLSHcode` is retrievable through `curl_share_errno()`.
pub(crate) const SHARE_SETOPT_REFUSED: i32 = 0;
/// [`elephc_curl_share_setopt`]'s answer: not a cURL share option at all -> php-src's own
/// `ValueError`.
pub(crate) const SHARE_SETOPT_INVALID: i32 = -1;

/// Allocates a libcurl share handle and registers it under a fresh id, or returns `0`
/// when libcurl could not allocate. PHP docs describe `curl_share_init()` as never
/// failing; the `false` this can still answer is the same DIVERGENCE `curl_init()`/
/// `curl_multi_init()` already document, handled the same way in the prelude (a thrown
/// `RuntimeException`).
#[no_mangle]
pub extern "C" fn elephc_curl_share_init() -> i64 {
    handles::ffi_guard(0, || {
        easy::ensure_global_init();
        let share = unsafe { curl_share_init() };
        if share.is_null() {
            return 0;
        }
        let id = next_share_id();
        handles::lock_recover(shares()).insert(
            id,
            ShareEntry {
                share,
                attached: Vec::new(),
                last_errno: CURLSHE_OK,
                persistent: false,
            },
        );
        id
    })
}

/// Applies an integer-valued `CURLSHOPT_*` option to share handle `share_id`.
///
/// Answers [`SHARE_SETOPT_APPLIED`], [`SHARE_SETOPT_REFUSED`] (a value libcurl itself
/// refused -> PHP `false`, no warning — see this module's header) or
/// [`SHARE_SETOPT_INVALID`] (not a cURL share option at all -> php-src's `ValueError`).
#[no_mangle]
pub extern "C" fn elephc_curl_share_setopt(share_id: i64, opt: i64, value: i64) -> i32 {
    handles::ffi_guard(SHARE_SETOPT_REFUSED, || {
        if share_option_kind(opt) != SHARE_OPTION_LONG {
            return SHARE_SETOPT_INVALID;
        }
        let mut guard = handles::lock_recover(shares());
        let Some(entry) = guard.get_mut(&share_id) else {
            return SHARE_SETOPT_REFUSED;
        };
        let code = unsafe { curl_share_setopt(entry.share, opt as c_int, value as c_long) };
        entry.last_errno = code;
        if code == CURLSHE_OK {
            SHARE_SETOPT_APPLIED
        } else {
            SHARE_SETOPT_REFUSED
        }
    })
}

/// Returns the `CURLSHcode` from the most recent share operation on `share_id` (`0` =
/// `CURLSHE_OK`), backing PHP's `curl_share_errno()`. `0` for an unknown id — the same
/// "nothing happened yet" answer a fresh handle's own `CURLSHE_OK` gives, matching
/// `curl_errno()`'s contract for a handle that never performed a transfer.
#[no_mangle]
pub extern "C" fn elephc_curl_share_errno(share_id: i64) -> i32 {
    handles::ffi_guard(CURLSHE_OK, || {
        let guard = handles::lock_recover(shares());
        guard
            .get(&share_id)
            .map_or(CURLSHE_OK, |entry| entry.last_errno)
    })
}

/// Copies libcurl's own message for a `CURLSHcode` into `out`, for PHP's
/// `curl_share_strerror()`. Handle-free, and a DIFFERENT numbering space from
/// `CURLcode`/`CURLMcode` — `curl_share_strerror(2)` ("share is in use") is unrelated to
/// `curl_strerror(2)`/`curl_multi_strerror(2)`.
///
/// # Safety
/// `out` must be valid for `out_cap` bytes when non-null. `out_len` must be valid for a
/// write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_share_strerror(
    code: i32,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        easy::ensure_global_init();
        let message = unsafe {
            let text = curl_share_strerror(code as CURLSHcode);
            if text.is_null() {
                Vec::new()
            } else {
                std::ffi::CStr::from_ptr(text).to_bytes().to_vec()
            }
        };
        unsafe { crate::abi::publish_bytes(&message, out, out_cap, out_len) }
    })
}

/// Attaches easy handle `easy_id` to share handle `share_id` via `CURLOPT_SHARE`, for
/// PHP's `curl_setopt($ch, CURLOPT_SHARE, $sh)`. Returns `1` on success, `0` for an
/// unknown easy or share id or a libcurl setopt failure — PHP's `curl_setopt()` contract
/// is a plain boolean, unlike the multi/share three-way status codes.
///
/// LIBCURL IS CALLED FIRST; the bridge's OWN `attached`/`share_id` bookkeeping is only
/// updated once `curl_easy_setopt` has actually accepted the pointer, so a failed call
/// leaves both sides exactly as they were. See this module's header for the full
/// re-attachment argument (why the easy handle is removed from any PREVIOUS share's
/// `attached` list here, not just added to the new one).
#[no_mangle]
pub extern "C" fn elephc_curl_easy_set_share(easy_id: i64, share_id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let Some(easy) = easy_ptr(easy_id) else {
            return 0;
        };
        let share_ptr = {
            let guard = handles::lock_recover(shares());
            let Some(entry) = guard.get(&share_id) else {
                return 0;
            };
            entry.share
        };
        let code = unsafe { easy::setopt_ptr(easy, easy::CURLOPT_SHARE, share_ptr as *mut c_void) };
        if code != easy::CURLE_OK {
            return 0;
        }
        let previous = {
            let mut guard = handles::lock_recover(handles::handles());
            let Some(entry) = guard.get_mut(&easy_id) else {
                // The easy id resolved a moment ago (`easy_ptr` above) but is gone now —
                // unreachable under this crate's single-threaded caller contract, kept
                // as a defensive no-op rather than an assumption.
                return 1;
            };
            let previous = entry.share_id;
            entry.share_id = Some(share_id);
            previous
        };
        if let Some(previous_id) = previous {
            if previous_id != share_id {
                let mut guard = handles::lock_recover(shares());
                if let Some(entry) = guard.get_mut(&previous_id) {
                    entry.attached.retain(|&id| id != easy_id);
                }
            }
        }
        let mut guard = handles::lock_recover(shares());
        if let Some(entry) = guard.get_mut(&share_id) {
            if !entry.attached.contains(&easy_id) {
                entry.attached.push(easy_id);
            }
        }
        1
    })
}

/// Removes easy id `easy_id` from share `share_id`'s `attached` bookkeeping without
/// touching libcurl at all. Called from `crate::abi::elephc_curl_easy_reset` (which has
/// already reset `CURLOPT_SHARE` to null at the libcurl level itself) and
/// `crate::abi::elephc_curl_easy_free` (whose easy handle is already gone). A no-op for
/// an unknown share id — in particular one already freed, which is safe precisely because
/// this never touches libcurl.
///
/// Not required for MEMORY SAFETY (`elephc_curl_share_free`'s own detach loop already
/// skips an id `easy_ptr` no longer resolves), but without it a long-lived share —
/// especially one from `curl_share_init_persistent()`, which is NEVER freed — would
/// accumulate one dead id per easy handle ever attached to it for the life of the
/// process.
pub(crate) fn detach_easy(share_id: i64, easy_id: i64) {
    let mut guard = handles::lock_recover(shares());
    if let Some(entry) = guard.get_mut(&share_id) {
        entry.attached.retain(|&id| id != easy_id);
    }
}

/// Removes share handle `share_id` from the table and runs `curl_share_cleanup` on it. A
/// no-op for an unknown/already-freed id, and — MORE IMPORTANTLY — a documented no-op for
/// a [`ShareEntry::persistent`] entry: `curl_share_init_persistent()`'s whole contract is
/// that the underlying share outlives every PHP-level reference to it, so its Mixed cell's
/// teardown must not actually release anything. See this module's header.
///
/// EVERY STILL-ATTACHED EASY HANDLE IS DETACHED FIRST (`CURLOPT_SHARE` -> null), which is
/// what makes freeing a share before the easy handles that reference it SAFE rather than
/// merely usually-fine: see this module's header for the full argument. An attached id
/// this table's easy table no longer knows (already freed) is skipped — its own
/// `curl_easy_cleanup` already ran with the share still alive, which is all libcurl needs.
#[no_mangle]
pub extern "C" fn elephc_curl_share_free(share_id: i64) {
    handles::ffi_guard((), || {
        let mut guard = handles::lock_recover(shares());
        let Some(entry) = guard.get(&share_id) else {
            return;
        };
        if entry.persistent {
            return;
        }
        let entry = guard.remove(&share_id).expect("checked present above");
        drop(guard);
        for easy_id in &entry.attached {
            if let Some(easy) = easy_ptr(*easy_id) {
                unsafe { easy::setopt_ptr(easy, easy::CURLOPT_SHARE, std::ptr::null_mut()) };
            }
        }
        unsafe { curl_share_cleanup(entry.share) };
    })
}

/// Builds (or finds) the process-lifetime share behind PHP 8.5's
/// `curl_share_init_persistent()`. `values` is a comma-separated list of decimal
/// `CURL_LOCK_DATA_*` ints (the prelude has already validated each one is a real PHP
/// `CURL_LOCK_DATA_*` constant before encoding it this way — see `crate::curl_prelude`'s
/// `curl_share_init_persistent()` body); an empty string is zero values, matching the
/// empty-array case.
///
/// Returns the SAME share id for an equivalent (sorted, deduplicated) value set every
/// time — php-src's own "keyed by the sorted option set" semantics — or `0` when libcurl
/// could not allocate a genuinely new share.
///
/// Named `elephc_curl_share_persistent_init`, NOT `..._init_persistent`, on purpose: the
/// latter would be a textual PREFIX of the `elephc_curl_share_init` C name (this module's
/// plain `curl_share_init()` entry point), which the runtime object's own
/// "never bare-references an `elephc_curl_*` symbol outside its slot name" test scans for
/// as a substring — `..._init` immediately followed by `..._persistent_fn` inside this
/// function's OWN slot symbol trips that check as a false collision. The PHP-visible name
/// (`curl_share_init_persistent()`) and every other identifier in this feature (the
/// `__elephc_curl_share_init_persistent` builtin, the `__rt_curl_share_init_persistent`
/// runtime label, neither of which collides the same way) are unaffected.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_share_persistent_init(ptr: *const u8, len: usize) -> i64 {
    handles::ffi_guard(0, || {
        let csv: &[u8] = if len == 0 {
            &[]
        } else if ptr.is_null() {
            return 0;
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        let Some(mut values) = parse_lock_data_csv(csv) else {
            return 0;
        };
        values.sort_unstable();
        values.dedup();

        if let Some(&existing) = handles::lock_recover(persistent_shares()).get(&values) {
            return existing;
        }

        easy::ensure_global_init();
        let share = unsafe { curl_share_init() };
        if share.is_null() {
            return 0;
        }
        for &value in &values {
            // Best-effort: every value reaching here was already validated as one of the
            // five real `CURL_LOCK_DATA_*` constants by the prelude, so a refusal here
            // would mean this particular libcurl build lacks that lock-data kind — not a
            // reason to discard the whole share, which still functions for the rest.
            unsafe { curl_share_setopt(share, 1, value as c_long) };
        }
        let id = next_share_id();
        handles::lock_recover(shares()).insert(
            id,
            ShareEntry {
                share,
                attached: Vec::new(),
                last_errno: CURLSHE_OK,
                persistent: true,
            },
        );
        handles::lock_recover(persistent_shares()).insert(values, id);
        id
    })
}

/// Parses a comma-separated list of decimal `i64`s. `b""` is zero values. `None` on any
/// malformed token (not expected once the prelude's own validation runs, but never trusted
/// blindly across the ABI boundary).
fn parse_lock_data_csv(csv: &[u8]) -> Option<Vec<i64>> {
    if csv.is_empty() {
        return Some(Vec::new());
    }
    let text = std::str::from_utf8(csv).ok()?;
    text.split(',').map(|token| token.parse::<i64>().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_option_kind_accepts_only_share_and_unshare() {
        assert_eq!(share_option_kind(1), SHARE_OPTION_LONG);
        assert_eq!(share_option_kind(2), SHARE_OPTION_LONG);
        for invalid in [0, 3, 4, 5, -1, 999_999] {
            assert_eq!(
                share_option_kind(invalid),
                SHARE_OPTION_INVALID,
                "{invalid} must not classify as a real cURL share option"
            );
        }
    }

    #[test]
    fn parse_lock_data_csv_parses_and_rejects() {
        assert_eq!(parse_lock_data_csv(b""), Some(Vec::new()));
        assert_eq!(parse_lock_data_csv(b"2"), Some(vec![2]));
        assert_eq!(parse_lock_data_csv(b"2,3,4"), Some(vec![2, 3, 4]));
        assert_eq!(parse_lock_data_csv(b"2,x,4"), None);
        assert_eq!(parse_lock_data_csv(b"not-a-number"), None);
    }
}
