//! Purpose:
//! The bridge half of PHP's `ext/curl` callback options: libcurl calls a C function
//! pointer mid-transfer, and this module turns that into a call back into compiled PHP.
//! Holds the per-handle callback slots, the six libcurl trampolines, and the marshalling
//! of each callback's arguments into the shape the codegen adapter expects.
//!
//! Called from:
//! - libcurl itself, from inside `curl_easy_perform` / `curl_multi_perform` (the
//!   trampolines are registered by [`apply_callback`]).
//! - `crate::abi::elephc_curl_easy_set_callback`, the single entry point the curl
//!   prelude uses to install, replace, or clear a callback.
//! - `crate::abi::elephc_curl_easy_reset` / `_duphandle` / `_free`, for slot lifecycle.
//!
//! Key details:
//! - **This crate never invokes PHP itself.** It stores three opaque words per slot —
//!   the callable's descriptor pointer, the owning `CurlHandle` object pointer, and the
//!   address of a codegen-emitted adapter (`__rt_curl_invoke_callback`) — all handed
//!   down from the prelude, and calls the adapter through the stored address. That keeps
//!   the "shared runtime never references `elephc_curl_*`" boundary intact in both
//!   directions: the runtime adapter is reached by address, never by symbol name, and
//!   this crate declares no `__rt_*` extern.
//! - **Descriptor ownership lives in PHP.** The prelude roots the normalized callable in
//!   a `CurlHandle` property, so the descriptor outlives every transfer without this
//!   crate ever touching a refcount it cannot see. The slots here are pure borrows;
//!   clearing one frees nothing.
//! - **The `CurlHandle` object pointer is a non-owning back-pointer**, exactly like
//!   php-src's `ch->self`. It cannot dangle: the object's own teardown is what calls
//!   `elephc_curl_easy_free`, so the entry holding the pointer dies with the object it
//!   points at, and a callback can only run while a `curl_exec()` frame holds the object
//!   alive. Storing an owning reference instead would be a refcount cycle
//!   (object → property → slot → object) that elephc, having no cycle collector, would
//!   never break.
//! - **No lock is held across the PHP call.** Every trampoline copies the slot out of
//!   the handle table, drops the guard, and only then calls the adapter — PHP code
//!   running inside the callback is free to call `curl_setopt()` / `curl_exec()` on
//!   OTHER handles, which re-enters this table. Same-handle re-entrancy stays UB, as it
//!   already is for libcurl itself.
//! - **A PHP exception never unwinds through libcurl.** The adapter installs its own
//!   `setjmp` firewall and reports a throw as `status = -1`; the trampoline turns that
//!   into libcurl's abort code for its callback kind and records
//!   `EasyEntry::callback_threw` so `elephc_curl_easy_perform` leaves `curl_errno()`
//!   alone (measured php-src behavior: an exception out of a write callback surfaces as
//!   a throw with `curl_errno() === 0`). The runtime re-raises the pending exception
//!   once `curl_easy_perform` has returned.

use std::ffi::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::easy::{self, CURL};
use crate::handles::{self, EasyEntry};

/// Slot index for `CURLOPT_WRITEFUNCTION` (20011).
pub(crate) const SLOT_WRITE: i32 = 0;
/// Slot index for `CURLOPT_HEADERFUNCTION` (20079).
pub(crate) const SLOT_HEADER: i32 = 1;
/// Slot index for `CURLOPT_READFUNCTION` (20012).
pub(crate) const SLOT_READ: i32 = 2;
/// Slot index for `CURLOPT_PROGRESSFUNCTION` (20056).
pub(crate) const SLOT_PROGRESS: i32 = 3;
/// Slot index for `CURLOPT_DEBUGFUNCTION` (20094).
pub(crate) const SLOT_DEBUG: i32 = 4;
/// Slot index for `CURLOPT_XFERINFOFUNCTION` (20219). Kept apart from
/// [`SLOT_PROGRESS`] because php-src keeps two separate handler records and libcurl
/// gives `CURLOPT_XFERINFOFUNCTION` precedence when both are registered; merging them
/// would make "set progress last" win, which is not what either does.
pub(crate) const SLOT_XFERINFO: i32 = 5;
/// Number of callback slots on an easy handle.
pub(crate) const SLOT_COUNT: usize = 6;

/// `CURL_READFUNC_ABORT`: returned by a read callback to fail the transfer with
/// `CURLE_ABORTED_BY_CALLBACK` (libcurl 8.21.0, `include/curl/curl.h`).
const CURL_READFUNC_ABORT: usize = 0x1000_0000;

/// `CURLOPT_HEADERFUNCTION` (20079) / `CURLOPT_HEADERDATA` (10029).
const CURLOPT_HEADERFUNCTION: c_int = 20079;
const CURLOPT_HEADERDATA: c_int = 10029;
/// `CURLOPT_READFUNCTION` (20012) / `CURLOPT_READDATA` (10009).
const CURLOPT_READFUNCTION: c_int = 20012;
const CURLOPT_READDATA: c_int = 10009;
/// `CURLOPT_XFERINFOFUNCTION` (20219) / `CURLOPT_XFERINFODATA` (10057).
const CURLOPT_XFERINFOFUNCTION: c_int = 20219;
const CURLOPT_XFERINFODATA: c_int = 10057;
/// `CURLOPT_DEBUGFUNCTION` (20094) / `CURLOPT_DEBUGDATA` (10095). The data option is
/// `CURLOPTTYPE_CBPOINT + 95`, NOT `+ 94`: it is the one pair in this list whose function
/// and data numbers do not share an offset (pinned libcurl 8.21.0,
/// `include/curl/curl.h:1475,1478`). Getting it wrong is silent — libcurl accepts the
/// write to option 10094 (`CURLOPT_PREQUOTE`'s neighbour) and the debug callback then
/// receives a `userdata` that matches no handle, so it never calls PHP at all.
const CURLOPT_DEBUGFUNCTION: c_int = 20094;
const CURLOPT_DEBUGDATA: c_int = 10095;

/// `result_kind`: the PHP return value is read as an integer (`zval_get_long`).
const RESULT_INT: i64 = 0;
/// `result_kind`: the PHP return value is read as a string and copied into
/// `CallSpec::out_buf`, capped at `out_cap` (php-src's `MIN(size * nmemb, len)`).
const RESULT_STRING: i64 = 1;

/// Runtime value tags shared with `__rt_mixed_from_value`. Only the four the
/// callback surface needs are named here.
const TAG_INT: i64 = 0;
const TAG_STRING: i64 = 1;
const TAG_NULL: i64 = 8;

/// One PHP argument, in the `(tag, lo, hi)` triple shape `__rt_mixed_from_value`
/// consumes. `TAG_STRING` puts the byte pointer in `lo` and the byte length in `hi`;
/// the adapter deep-copies the bytes, so a transient libcurl buffer is safe here.
///
/// The layout is load-bearing: the adapter indexes this array with a 24-byte stride
/// and reads `tag@0`, `lo@8`, `hi@16`.
#[repr(C)]
pub(crate) struct CallArg {
    tag: i64,
    lo: i64,
    hi: i64,
}

/// The call description handed to the codegen adapter. Field offsets are load-bearing
/// (the adapter reads them as raw displacements): `argc@0`, `argv@8`, `result_kind@16`,
/// `out_buf@24`, `out_cap@32`, `out_len@40`, `status@48`.
///
/// `argv` describes the arguments AFTER `$ch`; the adapter always passes the
/// `CurlHandle` object as argument 0 itself, because only it can box an object.
#[repr(C)]
pub(crate) struct CallSpec {
    argc: i64,
    argv: *const CallArg,
    result_kind: i64,
    out_buf: *mut u8,
    out_cap: i64,
    /// Written by the adapter: bytes copied into `out_buf` for `RESULT_STRING`.
    out_len: i64,
    /// Written by the adapter: `0` on a normal return, `-1` when the PHP callable
    /// threw and the adapter's firewall caught the `longjmp`.
    status: i64,
}

/// The codegen adapter's C ABI: `(descriptor, curl_handle_object, spec) -> i64`.
/// The returned integer is the PHP callable's return value cast to `int` for
/// [`RESULT_INT`]; for [`RESULT_STRING`] it is unused and `spec.out_len` carries the
/// answer instead.
type CallbackAdapter =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut CallSpec) -> i64;

/// One installed PHP callback: everything the trampolines need, and nothing they own.
///
/// All three words come from the prelude and are borrowed, never freed here — see the
/// module doc for why the `CurlHandle` back-pointer in particular must stay non-owning.
#[derive(Clone, Copy)]
pub(crate) struct CallbackSlot {
    /// The normalized callable's 64-byte descriptor, rooted in a PHP property.
    pub(crate) descriptor: *mut c_void,
    /// The `CurlHandle` object this callback belongs to, passed to PHP as `$ch`.
    pub(crate) self_obj: *mut c_void,
    /// Address of `__rt_curl_invoke_callback`, obtained by the prelude through
    /// `__elephc_curl_adapter_addr()`.
    pub(crate) adapter: *const c_void,
}

impl CallbackSlot {
    /// An empty slot. `descriptor.is_null()` is the "no PHP callback here" test.
    pub(crate) const fn empty() -> Self {
        Self {
            descriptor: std::ptr::null_mut(),
            self_obj: std::ptr::null_mut(),
            adapter: std::ptr::null(),
        }
    }

    /// Whether this slot holds a callable the trampolines should invoke.
    pub(crate) fn is_set(&self) -> bool {
        !self.descriptor.is_null() && !self.adapter.is_null()
    }
}

/// Calls the PHP callable in `slot` with `$ch` plus `args`, reading the result
/// according to `result_kind`. Returns `(result, out_len, threw)`.
///
/// # Safety
/// `slot` must be set (`CallbackSlot::is_set`), its adapter address must be the one the
/// prelude obtained from `__elephc_curl_adapter_addr()`, and no handle-table lock may be
/// held: the PHP callable can re-enter this crate on other handles.
unsafe fn invoke(
    slot: CallbackSlot,
    args: &[CallArg],
    result_kind: i64,
    out_buf: *mut u8,
    out_cap: i64,
) -> (i64, i64, bool) {
    let mut spec = CallSpec {
        argc: args.len() as i64,
        argv: args.as_ptr(),
        result_kind,
        out_buf,
        out_cap,
        out_len: 0,
        status: 0,
    };
    // SAFETY: the address came from `__elephc_curl_adapter_addr()`, whose only possible
    // value is the codegen adapter emitted with exactly this signature.
    let adapter: CallbackAdapter = std::mem::transmute(slot.adapter);
    let result = adapter(slot.descriptor, slot.self_obj, &mut spec);
    (result, spec.out_len, spec.status != 0)
}

/// Set whenever a PHP callback's `longjmp` was caught by the adapter's firewall, and
/// cleared by [`take_callback_threw`] once the runtime has consumed it.
///
/// THIS, NOT `_exc_value`, IS WHAT AUTHORIZES THE RUNTIME TO RE-RAISE. An earlier design
/// had `__rt_curl_rethrow_pending` re-raise whenever `_exc_value` was non-null, which is
/// wrong in both directions: a `curl_exec()` called from a `finally` block that is running
/// with an unmatched throw still pending would re-raise that unrelated exception and
/// truncate the `finally` (php runs it to the end), and a callback whose own `try/catch`
/// cleared `_exc_value` would make a genuine callback throw vanish. A flag the bridge
/// itself sets is the only signal that actually means "a callback threw during this
/// transfer".
///
/// Process-wide rather than per-handle because the multi interface's re-raise site
/// (`__rt_curl_multi_exec`) is handed a MULTI id and the flag is raised on an EASY one;
/// compiled PHP is effectively single-threaded, so at most one transfer is in flight.
static CALLBACK_THREW: AtomicBool = AtomicBool::new(false);

/// Reads the slot at `index` for `id` out of the handle table and drops the guard.
/// Returns `None` for an unknown id, an empty slot, or a transfer in which a callback has
/// ALREADY thrown.
///
/// That last case is load-bearing. A write callback that throws aborts the transfer, but
/// libcurl still runs its teardown — and with `CURLOPT_VERBOSE` on, teardown `infof()`
/// calls reach the debug callback. Invoking PHP again there would run user code with the
/// first exception still pending, and a `try/catch` inside that second callback would
/// silently swallow it. Nothing is invoked after the first throw for the rest of the
/// transfer.
///
/// This is deliberately keyed on the per-handle flag rather than on `_exc_value`: see
/// [`CALLBACK_THREW`] for why a pending `_exc_value` does NOT imply "a callback threw".
///
/// Every trampoline starts here: the table lock must not be held across the PHP call.
fn take_slot(id: i64, index: usize) -> Option<CallbackSlot> {
    let guard = handles::lock_recover(handles::handles());
    let entry = guard.get(&id)?;
    if entry.callback_threw {
        return None;
    }
    let slot = entry.callbacks[index];
    slot.is_set().then_some(slot)
}

/// Records that a PHP callback threw: on the handle, so `elephc_curl_easy_perform` can
/// report the failure without inventing a `CURLcode`, and process-wide, so the runtime
/// knows it is allowed to re-raise the parked throwable.
fn mark_threw(id: i64) {
    CALLBACK_THREW.store(true, Ordering::SeqCst);
    let mut guard = handles::lock_recover(handles::handles());
    if let Some(entry) = guard.get_mut(&id) {
        entry.callback_threw = true;
    }
}

/// Reports whether a PHP callback threw since the last time this was asked, clearing the
/// flag. Called by `__rt_curl_rethrow_pending` right after `curl_easy_perform` /
/// `curl_multi_perform` / `curl_easy_pause` returns; a `1` is what authorizes the runtime
/// to resume the throw that the adapter's firewall parked.
#[no_mangle]
pub extern "C" fn elephc_curl_take_callback_threw() -> i32 {
    handles::ffi_guard(0, || CALLBACK_THREW.swap(false, Ordering::SeqCst) as i32)
}

/// Runs the write/header PHP callback for `id` over `chunk` and returns what libcurl
/// should see: the callable's integer return value, which libcurl compares against
/// `chunk.len()` and treats as `CURLE_WRITE_ERROR` when they differ.
///
/// # Safety
/// No handle-table lock may be held.
pub(crate) unsafe fn run_bytes_callback(id: i64, index: usize, chunk: &[u8]) -> usize {
    let Some(slot) = take_slot(id, index) else {
        return 0;
    };
    let args = [CallArg {
        tag: TAG_STRING,
        lo: chunk.as_ptr() as i64,
        hi: chunk.len() as i64,
    }];
    let (result, _, threw) = invoke(slot, &args, RESULT_INT, std::ptr::null_mut(), 0);
    if threw {
        mark_threw(id);
        // Any value other than the chunk length aborts with CURLE_WRITE_ERROR; 0 is
        // the conventional "wrote nothing" form of that.
        return 0;
    }
    // A negative return would wrap to a huge `usize` and read as "wrote more than we
    // gave you", which libcurl also treats as a failure — but clamping keeps the
    // reported cause honest.
    if result < 0 {
        return 0;
    }
    result as usize
}

/// The `CURLOPT_HEADERFUNCTION` trampoline: one call per response header line
/// (including the status line and the terminating empty line).
///
/// # Safety
/// Called by libcurl with `buffer` valid for `size * nitems` bytes. Must not unwind.
unsafe extern "C" fn header_callback(
    buffer: *mut c_char,
    size: usize,
    nitems: usize,
    userdata: *mut c_void,
) -> usize {
    let total = size.saturating_mul(nitems);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if total == 0 {
            return 0usize;
        }
        let chunk = std::slice::from_raw_parts(buffer as *const u8, total);
        run_bytes_callback(userdata as i64, SLOT_HEADER as usize, chunk)
    }));
    outcome.unwrap_or(0)
}

/// The `CURLOPT_READFUNCTION` trampoline: libcurl asks for up to `size * nitems`
/// upload bytes and the PHP callable answers with a string.
///
/// php-src truncates a longer string with `MIN` rather than failing (measured against
/// PHP 8.4.20), and treats a non-string return as end-of-data; both are reproduced by
/// the adapter writing `out_len = 0` for a non-string and capping at `out_cap`.
///
/// # Safety
/// Called by libcurl with `buffer` writable for `size * nitems` bytes. Must not unwind.
unsafe extern "C" fn read_callback(
    buffer: *mut c_char,
    size: usize,
    nitems: usize,
    userdata: *mut c_void,
) -> usize {
    let total = size.saturating_mul(nitems);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let id = userdata as i64;
        let Some(slot) = take_slot(id, SLOT_READ as usize) else {
            return 0usize;
        };
        // php-src passes the `CURLOPT_INFILE` stream as `$fd`. elephc rejects every
        // stream option in this build, so there is never a resource to pass and the
        // argument is always null — which is also what php-src passes when no
        // `CURLOPT_INFILE` was set.
        let args = [
            CallArg {
                tag: TAG_NULL,
                lo: 0,
                hi: 0,
            },
            CallArg {
                tag: TAG_INT,
                lo: total as i64,
                hi: 0,
            },
        ];
        let (_, out_len, threw) = invoke(
            slot,
            &args,
            RESULT_STRING,
            buffer as *mut u8,
            total as i64,
        );
        if threw {
            mark_threw(id);
            return CURL_READFUNC_ABORT;
        }
        out_len.max(0) as usize
    }));
    outcome.unwrap_or(CURL_READFUNC_ABORT)
}

/// The `CURLOPT_XFERINFOFUNCTION` trampoline, shared by PHP's
/// `CURLOPT_PROGRESSFUNCTION` and `CURLOPT_XFERINFOFUNCTION`.
///
/// Registered as the `curl_off_t` (xferinfo) form because it is the one libcurl gives
/// precedence to, but the four counters reach PHP as `int`s either way — php-src passes
/// `ZVAL_LONG` for both (measured: `gettype()` is `"integer"` for both options).
/// Slot precedence matches libcurl's own: an installed xferinfo callback wins over a
/// progress callback.
///
/// # Safety
/// Called by libcurl during a transfer. Must not unwind.
unsafe extern "C" fn xferinfo_callback(
    userdata: *mut c_void,
    dltotal: i64,
    dlnow: i64,
    ultotal: i64,
    ulnow: i64,
) -> c_int {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let id = userdata as i64;
        let slot = take_slot(id, SLOT_XFERINFO as usize)
            .or_else(|| take_slot(id, SLOT_PROGRESS as usize));
        let Some(slot) = slot else {
            return 0;
        };
        let args = [
            CallArg {
                tag: TAG_INT,
                lo: dltotal,
                hi: 0,
            },
            CallArg {
                tag: TAG_INT,
                lo: dlnow,
                hi: 0,
            },
            CallArg {
                tag: TAG_INT,
                lo: ultotal,
                hi: 0,
            },
            CallArg {
                tag: TAG_INT,
                lo: ulnow,
                hi: 0,
            },
        ];
        let (result, _, threw) = invoke(slot, &args, RESULT_INT, std::ptr::null_mut(), 0);
        if threw {
            mark_threw(id);
            // Nonzero aborts the transfer with CURLE_ABORTED_BY_CALLBACK (42).
            return 1;
        }
        (result != 0) as c_int
    }));
    outcome.unwrap_or(1)
}

/// The `CURLOPT_DEBUGFUNCTION` trampoline. Only fires while `CURLOPT_VERBOSE` is on —
/// php-src does not turn it on for you, and neither does this (measured: with
/// `CURLOPT_VERBOSE` unset the callback is never called).
///
/// The PHP return value is ignored, exactly as in php-src, which always answers `0`.
///
/// # Safety
/// Called by libcurl with `data` valid for `size` bytes. Must not unwind.
unsafe extern "C" fn debug_callback(
    _curl: *mut CURL,
    info_type: c_int,
    data: *mut c_char,
    size: usize,
    userdata: *mut c_void,
) -> c_int {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let id = userdata as i64;
        let Some(slot) = take_slot(id, SLOT_DEBUG as usize) else {
            return 0;
        };
        let bytes = if data.is_null() || size == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(data as *const u8, size)
        };
        let args = [
            CallArg {
                tag: TAG_INT,
                lo: info_type as i64,
                hi: 0,
            },
            CallArg {
                tag: TAG_STRING,
                lo: bytes.as_ptr() as i64,
                hi: bytes.len() as i64,
            },
        ];
        let (_, _, threw) = invoke(slot, &args, RESULT_INT, std::ptr::null_mut(), 0);
        if threw {
            mark_threw(id);
        }
        0
    }));
    outcome.unwrap_or(0)
}

/// Installs the read trampoline and its `userdata` on `curl`. Called from
/// `crate::abi::elephc_curl_easy_init` (and again from [`apply_registration`]) so that
/// EVERY handle carries it from creation, exactly as `install_write_callback` does for the
/// write side.
///
/// Installing it at creation is not tidiness, it is the fix for a real leak of the host's
/// stdin: libcurl's DEFAULT read function is `fread` reading `CURLOPT_READDATA`, whose
/// default is `stdin`. A fresh handle set to `CURLOPT_UPLOAD` with no PHP read callback
/// would therefore upload the PROCESS'S OWN STANDARD INPUT — and behave differently from
/// the very same handle after a `curl_reset()`, which used to be the only path that
/// installed this trampoline. php-src has no such window: it installs its `curl_read` in
/// `curl_init()`.
///
/// # Safety
/// `curl` must be a still-live pointer from `easy::init` belonging to `id`.
pub(crate) unsafe fn install_read_callback(curl: *mut CURL, id: i64) {
    let _ = easy::setopt_bytes_function(curl, CURLOPT_READFUNCTION, Some(read_callback));
    // `CURLOPT_READDATA` is an opaque `void *` this crate controls both ends of: it is
    // never dereferenced as a pointer, only cast back to the `i64` id it started as.
    let _ = easy::setopt_ptr(curl, CURLOPT_READDATA, id as usize as *mut c_void);
}

/// Installs or removes the libcurl-side registration for `index` on `curl`, according
/// to whether the handle's slots now hold a callable.
///
/// The write slot is deliberately absent: `crate::php_layer::install_write_callback`
/// keeps elephc's own write callback registered unconditionally (the default stdout and
/// `RETURNTRANSFER` paths need it too), and it consults the slot itself.
///
/// # Safety
/// `curl` must be a still-live pointer from `easy::init` belonging to `id`.
unsafe fn apply_registration(curl: *mut CURL, id: i64, slots: &[CallbackSlot; SLOT_COUNT]) {
    let userdata = id as usize as *mut c_void;

    if slots[SLOT_HEADER as usize].is_set() {
        let _ = easy::setopt_bytes_function(curl, CURLOPT_HEADERFUNCTION, Some(header_callback));
        let _ = easy::setopt_ptr(curl, CURLOPT_HEADERDATA, userdata);
    } else {
        let _ = easy::setopt_bytes_function(curl, CURLOPT_HEADERFUNCTION, None);
        let _ = easy::setopt_ptr(curl, CURLOPT_HEADERDATA, std::ptr::null_mut());
    }

    // THE READ SLOT IS NEVER DEREGISTERED, and `CURLOPT_READDATA` is never set to null.
    // Both would be memory-unsafe: libcurl's DEFAULT read function is `fread`, and it
    // reads `CURLOPT_READDATA` as the `FILE *` to read from (default `stdin`). Clearing
    // the function back to that default while leaving our handle id in `READDATA` would
    // hand `fread` a small integer as a `FILE *`; clearing `READDATA` to null instead
    // would hand it `NULL`. Either one segfaults the moment an upload runs — and
    // `elephc_curl_easy_reset` calls `clear_all` AFTER `curl_easy_reset` has restored
    // `stdin`, so a plain `curl_reset($ch)` followed by an upload would have been enough
    // to trigger it.
    //
    // Instead the trampoline stays installed with the handle's own id and reports
    // end-of-data (`0`) whenever the slot is empty. That is also exactly what php-src
    // does: it installs `curl_read` unconditionally at handle creation, and `curl_read`
    // with no PHP callable and no `fp` returns `0`.
    install_read_callback(curl, id);

    if slots[SLOT_PROGRESS as usize].is_set() || slots[SLOT_XFERINFO as usize].is_set() {
        let _ = easy::setopt_xferinfo_function(
            curl,
            CURLOPT_XFERINFOFUNCTION,
            Some(xferinfo_callback),
        );
        let _ = easy::setopt_ptr(curl, CURLOPT_XFERINFODATA, userdata);
    } else {
        let _ = easy::setopt_xferinfo_function(curl, CURLOPT_XFERINFOFUNCTION, None);
        let _ = easy::setopt_ptr(curl, CURLOPT_XFERINFODATA, std::ptr::null_mut());
    }

    if slots[SLOT_DEBUG as usize].is_set() {
        let _ = easy::setopt_debug_function(curl, CURLOPT_DEBUGFUNCTION, Some(debug_callback));
        let _ = easy::setopt_ptr(curl, CURLOPT_DEBUGDATA, userdata);
    } else {
        let _ = easy::setopt_debug_function(curl, CURLOPT_DEBUGFUNCTION, None);
        let _ = easy::setopt_ptr(curl, CURLOPT_DEBUGDATA, std::ptr::null_mut());
    }
}

/// Installs (`descriptor` non-null) or clears (`descriptor` null) the callback in
/// `index` on `entry`, keeping the libcurl-side registration in step.
///
/// The write slot additionally drives `EasyEntry::write_user` and `return_transfer`,
/// which together model php-src's single `handlers.write->method` enum: setting a write
/// callback selects `PHP_CURL_USER`, clearing it falls back to `PHP_CURL_STDOUT` (NOT to
/// whatever `CURLOPT_RETURNTRANSFER` was — measured), and `CURLOPT_RETURNTRANSFER`
/// selects `PHP_CURL_RETURN` and deselects the callback.
pub(crate) fn apply_callback(entry: &mut EasyEntry, index: usize, slot: CallbackSlot) {
    entry.callbacks[index] = slot;
    if index == SLOT_WRITE as usize {
        entry.write_user = slot.is_set();
        entry.return_transfer = false;
    }
    // SAFETY: `entry.curl` is this entry's live handle; the caller holds the table lock,
    // and libcurl option writes are not re-entrant into this crate.
    unsafe { apply_registration(entry.curl, entry.id, &entry.callbacks) };
}

/// Drops every callback slot and re-applies the (now empty) libcurl registration set.
/// Used by `curl_reset()` (php-src frees the handler callables there too) and by
/// `curl_copy_handle()`, whose duplicate must never inherit the ORIGINAL's `userdata` —
/// that is the original's handle id, so the copy's transfers would call back with the
/// wrong `$ch` and the wrong slots.
///
/// "Empty registration set" does not mean "no registrations": the read trampoline stays
/// installed and answers end-of-data, for the memory-safety reason spelled out in
/// [`apply_registration`]. Only the PHP-visible slots are dropped.
pub(crate) fn clear_all(entry: &mut EasyEntry) {
    entry.callbacks = [CallbackSlot::empty(); SLOT_COUNT];
    entry.write_user = false;
    // SAFETY: as in `apply_callback`.
    unsafe { apply_registration(entry.curl, entry.id, &entry.callbacks) };
}
