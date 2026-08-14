//! Purpose:
//! PHP's curl MULTI interface end to end inside the bridge: the raw
//! `curl_multi_*` libcurl bindings, the `i64`-keyed multi-handle table (its own
//! id space, mirroring the easy table in `crate::handles`), the frozen
//! `CURLMOPT_*` classification, and the `elephc_curl_multi_*` C ABI the
//! compiled prelude calls.
//!
//! Called from:
//! - Compiled PHP programs, through the `__rt_curl_multi_*` runtime helpers.
//! - `crate::tests`' gated real-libcurl tests.
//!
//! Key details:
//! - **THE TABLE LOCK IS NEVER HELD ACROSS `curl_multi_perform`/`curl_multi_wait`.**
//!   Those two drive the attached EASY handles, so libcurl calls this crate's own
//!   write callback (`crate::php_layer::write_callback`) from inside them, and that
//!   callback locks the EASY table. Holding the multi lock is not itself the
//!   deadlock — holding the easy one would be — but both functions can block for a
//!   whole timeout, so the pointer is copied out and the lock dropped first, exactly
//!   as `elephc_curl_easy_perform` documents. The same caller contract applies here:
//!   elephc-compiled programs are single-threaded, and no two `elephc_curl_*` calls
//!   for the same id may run concurrently.
//! - **ONE MESSAGE AT A TIME.** `curl_multi_info_read` pops one message off
//!   libcurl's queue, which is destructive: the message cannot be re-read. The ABI
//!   therefore PARKS the popped message in the entry
//!   ([`MultiEntry::last_message`]) and hands its four fields back one call at a
//!   time ([`elephc_curl_multi_info_read`]'s `field` argument), so the prelude can
//!   build PHP's `['msg' => …, 'result' => …, 'handle' => …]` array out of plain
//!   integers without this crate ever needing to know what a PHP array is.
//! - **ATTACHED EASY IDS ARE TRACKED** ([`MultiEntry::attached`]) for exactly one
//!   reason: `elephc_curl_multi_free` must detach them before `curl_multi_cleanup`.
//!   The PHP-VISIBLE add-order list lives on the `CurlMultiHandle` object instead
//!   (it has to: it holds the PHP objects, and their identity is what
//!   `curl_multi_info_read()` must return).
//! - **`elephc_curl_multi_add` RESETS THE EASY HANDLE'S CAPTURE BUFFER**, matching
//!   php-src's `curl_multi_add_handle`, which calls `_php_curl_cleanup_handle(ch)`
//!   for the same reason: without it, a handle reused across two multi runs would
//!   report the concatenation of both bodies from `curl_multi_getcontent()`.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_long, c_uint, c_void};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::easy::{self, CURL};
use crate::handles;

/// Opaque libcurl multi-handle type, named after libcurl's own `CURLM` typedef.
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum CURLM {}

/// libcurl's `CURLMcode`. `0` is `CURLM_OK`; `-1` is `CURLM_CALL_MULTI_PERFORM`
/// (never returned by a modern libcurl, but part of the type's range, which is
/// why every `CURLMcode` this file hands to PHP travels SIGNED).
pub(crate) type CURLMcode = c_int;

/// `CURLM_OK`: the multi operation succeeded.
const CURLM_OK: CURLMcode = 0;
/// `CURLM_BAD_HANDLE` (1): the answer for an id this table does not know, so a
/// caller can never read "nothing happened" as success.
const CURLM_BAD_HANDLE: CURLMcode = 1;

extern "C" {
    /// Allocates a new multi handle, or returns null on failure.
    fn curl_multi_init() -> *mut CURLM;

    /// Frees a multi handle. Every still-attached easy handle is removed from it
    /// first by this crate (see [`elephc_curl_multi_free`]).
    fn curl_multi_cleanup(multi: *mut CURLM) -> CURLMcode;

    /// Attaches an easy handle to a multi handle.
    fn curl_multi_add_handle(multi: *mut CURLM, easy: *mut CURL) -> CURLMcode;

    /// Detaches an easy handle from a multi handle.
    fn curl_multi_remove_handle(multi: *mut CURLM, easy: *mut CURL) -> CURLMcode;

    /// Drives every attached transfer as far as it can go without blocking,
    /// reporting how many are still running through `running_handles`.
    fn curl_multi_perform(multi: *mut CURLM, running_handles: *mut c_int) -> CURLMcode;

    /// Waits until an attached transfer is ready (or `timeout_ms` elapses),
    /// reporting how many file descriptors were ready through `numfds`. This is
    /// the function php-src's `curl_multi_select()` calls.
    fn curl_multi_wait(
        multi: *mut CURLM,
        extra_fds: *mut c_void,
        extra_nfds: c_uint,
        timeout_ms: c_int,
        numfds: *mut c_int,
    ) -> CURLMcode;

    /// Pops the next message off the multi handle's queue, or returns null when
    /// the queue is empty. The returned pointer is owned by libcurl and is only
    /// valid until the next call that touches this multi handle.
    fn curl_multi_info_read(multi: *mut CURLM, msgs_in_queue: *mut c_int) -> *mut CurlMsg;

    /// The single variadic `curl_multi_setopt` symbol, called only through the
    /// typed wrappers below.
    fn curl_multi_setopt(multi: *mut CURLM, option: c_int, ...) -> CURLMcode;

    /// libcurl's `'static` human-readable text for a `CURLMcode`.
    fn curl_multi_strerror(code: CURLMcode) -> *const c_char;
}

/// libcurl's `struct CURLMsg`, mirrored field for field (`curl/multi.h`).
#[repr(C)]
pub(crate) struct CurlMsg {
    /// Which kind of message this is; `CURLMSG_DONE` is the only one libcurl
    /// defines.
    msg: c_int,
    /// The easy handle the message is about.
    easy_handle: *mut CURL,
    /// libcurl's message-specific `data` union.
    data: CurlMsgData,
}

/// The `data` union of libcurl's `CURLMsg`. Only the `result` arm is ever read,
/// and only for `CURLMSG_DONE` — which is the only message libcurl produces.
#[repr(C)]
pub(crate) union CurlMsgData {
    /// The generic arm libcurl documents for future message kinds.
    whatever: *mut c_void,
    /// The transfer's final `CURLcode`, for `CURLMSG_DONE`.
    result: c_int,
}

/// One `CURLMSG_DONE` message, decoded into the plain integers PHP needs, parked
/// so [`elephc_curl_multi_info_read`] can hand back one field per call.
#[derive(Clone, Copy, Default)]
struct InfoMessage {
    /// `CURLMSG_DONE`.
    msg: i64,
    /// The transfer's final `CURLcode`.
    result: i64,
    /// The bridge id of the easy handle the message is about, or `0` when the
    /// message names a handle this table no longer knows.
    easy_id: i64,
    /// How many messages were still queued after this one was popped.
    queued: i64,
}

/// One tracked libcurl multi handle.
pub(crate) struct MultiEntry {
    /// The underlying libcurl multi handle, freed by `curl_multi_cleanup`.
    multi: *mut CURLM,
    /// The bridge ids of the easy handles currently attached, in add order.
    /// Used ONLY to detach them before `curl_multi_cleanup`; the PHP-visible
    /// add-order list lives on the `CurlMultiHandle` object.
    attached: Vec<i64>,
    /// The `CURLMcode` from the most recent multi operation on this handle,
    /// backing PHP's `curl_multi_errno()`.
    last_errno: CURLMcode,
    /// The most recently popped completion message (see this module's header).
    last_message: InfoMessage,
}

// SAFETY: identical reasoning to `crate::handles::EasyEntry`'s `Send` impl — the
// table mutex serializes access to the TABLE, and the single-threaded caller
// contract (documented in `crate::abi`'s module header and again at the top of
// this file) is what makes an individual `*mut CURLM` safe to drive.
unsafe impl Send for MultiEntry {}

/// The process-wide multi-handle table. A SEPARATE id space from the easy table:
/// a multi id and an easy id may both be `1`, and nothing ever passes one where
/// the other is expected — the ABI takes them as distinct parameters.
fn multis() -> &'static Mutex<HashMap<i64, MultiEntry>> {
    static MULTIS: OnceLock<Mutex<HashMap<i64, MultiEntry>>> = OnceLock::new();
    MULTIS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocates the next multi id. Monotonic, positive, never reused.
fn next_multi_id() -> i64 {
    static NEXT_ID: AtomicI64 = AtomicI64::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

/// Looks the raw `*mut CURL` for an easy id up in the easy table, or `None` when
/// the id is unknown (already freed, or never existed).
fn easy_ptr(id: i64) -> Option<*mut CURL> {
    let guard = handles::lock_recover(handles::handles());
    guard.get(&id).map(|entry| entry.curl)
}

/// Finds the bridge id of an easy handle from its raw pointer — the reverse
/// lookup `curl_multi_info_read` needs, since libcurl reports the completed
/// transfer by `*mut CURL` and PHP addresses handles by id.
fn easy_id_of(ptr: *mut CURL) -> i64 {
    let guard = handles::lock_recover(handles::handles());
    guard
        .iter()
        .find(|(_, entry)| entry.curl == ptr)
        .map_or(0, |(&id, _)| id)
}

/// Classifies a `curl_multi_setopt()` option number, the multi-side mirror of
/// `crate::options::option_kind` and a MEMORY-SAFETY boundary for the same
/// reason: `curl_multi_setopt` is variadic and reads its third argument purely
/// from the option's numeric range, so `CURLMOPT_PUSHFUNCTION` (20014) would read
/// a PHP integer as a function pointer and call it.
///
/// Returns [`MULTI_OPTION_LONG`], [`MULTI_OPTION_OFF_T`],
/// [`MULTI_OPTION_UNSUPPORTED`] or [`MULTI_OPTION_INVALID`].
pub(crate) fn multi_option_kind(opt: i64) -> i32 {
    // The frozen surface's nine `CURLMOPT_*` options
    // (`scripts/docs/curl_surface.json`, `option_kinds`), by number:
    //   3 PIPELINING, 6 MAXCONNECTS, 7 MAX_HOST_CONNECTIONS,
    //   8 MAX_PIPELINE_LENGTH, 13 MAX_TOTAL_CONNECTIONS,
    //   16 MAX_CONCURRENT_STREAMS                                   -> long
    //   30009 CONTENT_LENGTH_PENALTY_SIZE, 30010 CHUNK_LENGTH_PENALTY_SIZE -> off_t
    //   20014 PUSHFUNCTION                                          -> callback
    match opt {
        3 | 6 | 7 | 8 | 13 | 16 => MULTI_OPTION_LONG,
        30_009 | 30_010 => MULTI_OPTION_OFF_T,
        // A callback option php-src really does support, through machinery this
        // build does not have (an HTTP/2 server-push hook, and HTTP/2 is not built
        // in). `false` plus a warning, matching every other unsupported option here
        // — never an inert `true` and never a wild function pointer.
        20_014 => MULTI_OPTION_UNSUPPORTED,
        _ => MULTI_OPTION_INVALID,
    }
}

/// `multi_option_kind`: not a `curl_multi_setopt()` option at all.
pub(crate) const MULTI_OPTION_INVALID: i32 = 0;
/// `multi_option_kind`: `curl_multi_setopt(multi, opt, long)`.
pub(crate) const MULTI_OPTION_LONG: i32 = 1;
/// `multi_option_kind`: `curl_multi_setopt(multi, opt, curl_off_t)`.
pub(crate) const MULTI_OPTION_OFF_T: i32 = 2;
/// `multi_option_kind`: a real PHP multi option this build cannot carry.
pub(crate) const MULTI_OPTION_UNSUPPORTED: i32 = 3;

/// [`elephc_curl_multi_setopt`]'s answer: the option was applied.
pub(crate) const MULTI_SETOPT_APPLIED: i32 = 1;
/// [`elephc_curl_multi_setopt`]'s answer: a real option this build cannot carry,
/// or one libcurl refused -> PHP `false` plus a warning.
pub(crate) const MULTI_SETOPT_UNSUPPORTED: i32 = 0;
/// [`elephc_curl_multi_setopt`]'s answer: not a cURL multi option -> php-src's
/// own `ValueError`.
pub(crate) const MULTI_SETOPT_INVALID: i32 = -1;

/// [`elephc_curl_multi_info_read`]'s `field`: pop the next message, answering `1`
/// when there was one and `0` when the queue is empty.
pub(crate) const INFO_FIELD_ADVANCE: i64 = 0;
/// [`elephc_curl_multi_info_read`]'s `field`: the parked message's `CURLMSG`.
pub(crate) const INFO_FIELD_MSG: i64 = 1;
/// [`elephc_curl_multi_info_read`]'s `field`: the parked message's `CURLcode`.
pub(crate) const INFO_FIELD_RESULT: i64 = 2;
/// [`elephc_curl_multi_info_read`]'s `field`: the parked message's easy-handle id.
pub(crate) const INFO_FIELD_EASY_ID: i64 = 3;
/// [`elephc_curl_multi_info_read`]'s `field`: how many messages were left queued.
pub(crate) const INFO_FIELD_QUEUED: i64 = 4;

/// Allocates a libcurl multi handle and registers it under a fresh id, or
/// returns `0` when libcurl could not allocate.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_init() -> i64 {
    handles::ffi_guard(0, || {
        easy::ensure_global_init();
        let multi = unsafe { curl_multi_init() };
        if multi.is_null() {
            return 0;
        }
        let id = next_multi_id();
        handles::lock_recover(multis()).insert(
            id,
            MultiEntry {
                multi,
                attached: Vec::new(),
                last_errno: CURLM_OK,
                last_message: InfoMessage::default(),
            },
        );
        id
    })
}

/// Attaches easy handle `easy_id` to multi handle `multi_id`, returning
/// libcurl's `CURLMcode` (`0` = `CURLM_OK`), which is what PHP's
/// `curl_multi_add_handle()` returns. `CURLM_BAD_HANDLE` for an unknown id on
/// either side.
///
/// The easy handle's captured body and last error are CLEARED here, matching
/// php-src's `curl_multi_add_handle`, which runs `_php_curl_cleanup_handle(ch)`
/// before attaching: the buffer a multi transfer fills is read back by
/// `curl_multi_getcontent()`, and nothing else would ever reset it.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_add(multi_id: i64, easy_id: i64) -> i32 {
    handles::ffi_guard(CURLM_BAD_HANDLE, || {
        let Some(easy) = easy_ptr(easy_id) else {
            return record_errno(multi_id, CURLM_BAD_HANDLE);
        };
        {
            let mut guard = handles::lock_recover(handles::handles());
            if let Some(entry) = guard.get_mut(&easy_id) {
                entry.body.clear();
                entry.taken_body.clear();
                entry.last_errno = easy::CURLE_OK;
                entry.last_error.clear();
                entry.error_buf.iter_mut().for_each(|byte| *byte = 0);
            }
        }
        let mut guard = handles::lock_recover(multis());
        let Some(entry) = guard.get_mut(&multi_id) else {
            return CURLM_BAD_HANDLE;
        };
        let code = unsafe { curl_multi_add_handle(entry.multi, easy) };
        entry.last_errno = code;
        if code == CURLM_OK && !entry.attached.contains(&easy_id) {
            entry.attached.push(easy_id);
        }
        code
    })
}

/// Detaches easy handle `easy_id` from multi handle `multi_id`, returning
/// libcurl's `CURLMcode`.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_remove(multi_id: i64, easy_id: i64) -> i32 {
    handles::ffi_guard(CURLM_BAD_HANDLE, || {
        let Some(easy) = easy_ptr(easy_id) else {
            return record_errno(multi_id, CURLM_BAD_HANDLE);
        };
        let mut guard = handles::lock_recover(multis());
        let Some(entry) = guard.get_mut(&multi_id) else {
            return CURLM_BAD_HANDLE;
        };
        let code = unsafe { curl_multi_remove_handle(entry.multi, easy) };
        entry.last_errno = code;
        if code == CURLM_OK {
            entry.attached.retain(|&id| id != easy_id);
        }
        code
    })
}

/// Drives every attached transfer as far as it can go without blocking.
///
/// Returns BOTH of PHP's outputs packed into one `i64`: the still-running count
/// in the high 32 bits and the `CURLMcode` in the low 32 (signed, so
/// `CURLM_CALL_MULTI_PERFORM` = `-1` survives). PHP's `curl_multi_exec()` needs
/// the code as its return value and the count through a by-reference parameter,
/// and one packed integer answers both without a second entry point or a
/// pointer out-parameter — the same shape [`elephc_curl_multi_info_read`] uses
/// for the completion queue.
///
/// THE TABLE LOCK IS DROPPED BEFORE `curl_multi_perform`, which runs the attached
/// easy handles and therefore re-enters this crate's write callback (which locks
/// the EASY table). See this module's header for the full contract.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_perform(multi_id: i64) -> i64 {
    handles::ffi_guard(pack_perform(0, CURLM_BAD_HANDLE), || {
        let multi = {
            let guard = handles::lock_recover(multis());
            let Some(entry) = guard.get(&multi_id) else {
                return pack_perform(0, CURLM_BAD_HANDLE);
            };
            entry.multi
        };
        // Opens a fresh callback-throw scope, exactly as `elephc_curl_easy_perform` does.
        // Without it the process-wide gate raised by a throw in an EARLIER
        // `curl_multi_exec()` call would still be set here and would silently suppress
        // every callback on every attached handle for the rest of the program.
        crate::callbacks::begin_transfer();
        let mut running: c_int = 0;
        let code = unsafe { curl_multi_perform(multi, &mut running as *mut c_int) };
        record_errno(multi_id, code);
        pack_perform(running.max(0) as i64, code)
    })
}

/// Packs a `curl_multi_perform` result the way [`elephc_curl_multi_perform`]
/// documents: `running` in the high 32 bits, `code` in the low 32.
fn pack_perform(running: i64, code: CURLMcode) -> i64 {
    (running << 32) | i64::from(code as u32)
}

/// Waits up to `timeout_ms` for an attached transfer to become ready, answering
/// the number of ready file descriptors, or `-1` when libcurl reported an error
/// — exactly PHP's `curl_multi_select()` contract.
///
/// This is `curl_multi_wait`, the function php-src calls, NOT `curl_multi_poll`:
/// `wait` returns immediately when there is nothing waitable (so a loop that
/// forgets to call `curl_multi_exec()` spins, as it does in PHP), while `poll`
/// would sleep out the whole timeout and silently diverge from PHP's timing.
///
/// The lock is dropped before the call for the same reason
/// [`elephc_curl_multi_perform`] drops it: `curl_multi_wait` blocks.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_select(multi_id: i64, timeout_ms: i64) -> i32 {
    handles::ffi_guard(-1, || {
        let multi = {
            let guard = handles::lock_recover(multis());
            let Some(entry) = guard.get(&multi_id) else {
                return -1;
            };
            entry.multi
        };
        // A negative PHP timeout means "no timeout" to libcurl's own API, but
        // php-src casts the float straight to `int`, so a negative value reaches
        // libcurl unchanged; clamping only the OVERFLOW keeps that behaviour
        // while never handing libcurl a truncated wrap-around.
        let timeout = timeout_ms.clamp(c_int::MIN as i64, c_int::MAX as i64) as c_int;
        let mut numfds: c_int = 0;
        let code = unsafe {
            curl_multi_wait(
                multi,
                std::ptr::null_mut(),
                0,
                timeout,
                &mut numfds as *mut c_int,
            )
        };
        record_errno(multi_id, code);
        if code != CURLM_OK {
            return -1;
        }
        numfds
    })
}

/// Reads the multi handle's completion queue, one field per call.
///
/// `field` = [`INFO_FIELD_ADVANCE`] pops the next message off libcurl's queue
/// (destructively — libcurl cannot re-read it) and parks it on the entry,
/// answering `1` when there was a message and `0` when the queue is empty.
/// `field` = [`INFO_FIELD_MSG`]/[`INFO_FIELD_RESULT`]/[`INFO_FIELD_EASY_ID`]/
/// [`INFO_FIELD_QUEUED`] then read the parked message's fields, so the prelude
/// can assemble PHP's `['msg' => …, 'result' => …, 'handle' => …]` array out of
/// plain integers. An unknown `field` or id answers `0`.
///
/// The parked message stays valid until the next `INFO_FIELD_ADVANCE` on the
/// same multi handle — the same "valid until overwritten" convention
/// `elephc_curl_easy_take_body` uses.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_info_read(multi_id: i64, field: i64) -> i64 {
    handles::ffi_guard(0, || {
        if field != INFO_FIELD_ADVANCE {
            let guard = handles::lock_recover(multis());
            let Some(entry) = guard.get(&multi_id) else {
                return 0;
            };
            let message = entry.last_message;
            return match field {
                INFO_FIELD_MSG => message.msg,
                INFO_FIELD_RESULT => message.result,
                INFO_FIELD_EASY_ID => message.easy_id,
                INFO_FIELD_QUEUED => message.queued,
                _ => 0,
            };
        }
        let multi = {
            let guard = handles::lock_recover(multis());
            let Some(entry) = guard.get(&multi_id) else {
                return 0;
            };
            entry.multi
        };
        let mut queued: c_int = 0;
        // SAFETY: `multi` is a live handle from this table, and the returned
        // pointer is read (never freed) before any other call touches this multi
        // handle, which is libcurl's own validity window for a `CURLMsg *`.
        let message = unsafe { curl_multi_info_read(multi, &mut queued as *mut c_int) };
        if message.is_null() {
            return 0;
        }
        let (msg, result, easy_ptr) = unsafe {
            let message = &*message;
            (
                i64::from(message.msg),
                // The `result` arm is the only one libcurl ever fills for
                // `CURLMSG_DONE`, which is the only message it produces.
                i64::from(message.data.result),
                message.easy_handle,
            )
        };
        // The reverse pointer -> id lookup takes the EASY table's lock, so it
        // happens BEFORE the multi table is locked again below; taking them in
        // the other order anywhere would invite a lock-order inversion.
        let easy_id = easy_id_of(easy_ptr);
        let mut guard = handles::lock_recover(multis());
        let Some(entry) = guard.get_mut(&multi_id) else {
            return 0;
        };
        entry.last_message = InfoMessage {
            // libcurl's own `msg` verbatim, exactly as php-src's `add_assoc_long(…, "msg",
            // tmp_msg->msg)` reports it. `CURLMSG_DONE` is the only value libcurl has ever
            // produced, but fabricating it for some other message would be a lie about
            // what happened.
            msg,
            result,
            easy_id,
            queued: i64::from(queued.max(0)),
        };
        1
    })
}

/// Applies an integer-valued `CURLMOPT_*` option to multi handle `multi_id`.
///
/// Answers [`MULTI_SETOPT_APPLIED`], [`MULTI_SETOPT_UNSUPPORTED`] (a real PHP
/// option this build cannot carry, or one libcurl refused -> PHP `false` plus a
/// warning) or [`MULTI_SETOPT_INVALID`] (not a cURL multi
/// option at all -> php-src's `ValueError`). The three-way split is php-src's
/// own: its `curl_multi_setopt` ends with
/// `zend_argument_value_error(2, "is not a valid cURL multi option")`.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_setopt(multi_id: i64, opt: i64, value: i64) -> i32 {
    handles::ffi_guard(MULTI_SETOPT_UNSUPPORTED, || {
        let kind = multi_option_kind(opt);
        if kind == MULTI_OPTION_INVALID {
            return MULTI_SETOPT_INVALID;
        }
        if kind == MULTI_OPTION_UNSUPPORTED {
            return MULTI_SETOPT_UNSUPPORTED;
        }
        let mut guard = handles::lock_recover(multis());
        let Some(entry) = guard.get_mut(&multi_id) else {
            return MULTI_SETOPT_UNSUPPORTED;
        };
        // THE KIND, NOT THE VALUE, PICKS THE VARIADIC ARGUMENT TYPE: an `off_t`
        // option read as a `long` (or the reverse) is a wrong-width read inside
        // libcurl's own `va_arg`, which is why the table above exists at all.
        let code = match kind {
            MULTI_OPTION_OFF_T => unsafe { curl_multi_setopt(entry.multi, opt as c_int, value) },
            MULTI_OPTION_LONG => unsafe {
                curl_multi_setopt(entry.multi, opt as c_int, value as c_long)
            },
            _ => return MULTI_SETOPT_UNSUPPORTED,
        };
        entry.last_errno = code;
        if code == CURLM_OK {
            MULTI_SETOPT_APPLIED
        } else {
            MULTI_SETOPT_UNSUPPORTED
        }
    })
}

/// Returns the `CURLMcode` from the most recent multi operation on `multi_id`
/// (`0` = `CURLM_OK`), backing PHP's `curl_multi_errno()`. `CURLM_BAD_HANDLE`
/// for an unknown id.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_errno(multi_id: i64) -> i32 {
    handles::ffi_guard(CURLM_BAD_HANDLE, || {
        let guard = handles::lock_recover(multis());
        guard
            .get(&multi_id)
            .map_or(CURLM_BAD_HANDLE, |entry| entry.last_errno)
    })
}

/// Copies libcurl's own message for a `CURLMcode` into `out`, for PHP's
/// `curl_multi_strerror()`. Handle-free, and a DIFFERENT function from
/// `elephc_curl_strerror`: `CURLMcode` and `CURLcode` are separate numbering
/// spaces, so `curl_multi_strerror(3)` ("out of memory") and
/// `curl_strerror(3)` ("URL using bad/illegal format") are unrelated messages.
///
/// # Safety
/// `out` must be valid for `out_cap` bytes when non-null. `out_len` must be
/// valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_multi_strerror(
    code: i32,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        easy::ensure_global_init();
        let message = unsafe {
            let text = curl_multi_strerror(code as CURLMcode);
            if text.is_null() {
                Vec::new()
            } else {
                std::ffi::CStr::from_ptr(text).to_bytes().to_vec()
            }
        };
        unsafe { crate::abi::publish_bytes(&message, out, out_cap, out_len) }
    })
}

/// Removes multi handle `multi_id` from the table and runs `curl_multi_cleanup`
/// on it. A no-op for an unknown/already-freed id (ids are never reused).
///
/// EVERY STILL-ATTACHED EASY HANDLE IS DETACHED FIRST, which is libcurl's own
/// documented recommendation for `curl_multi_cleanup`. An attached handle whose
/// easy id this table no longer knows is skipped rather than guessed at: the easy
/// handle was freed first, and `curl_easy_cleanup` already detached it from this
/// multi handle itself (libcurl 8.21.0, `lib/url.c`'s `Curl_close`, which calls
/// `curl_multi_remove_handle` when the handle still belongs to a multi).
///
/// The table lock is dropped before the libcurl calls, matching
/// `elephc_curl_easy_free`.
#[no_mangle]
pub extern "C" fn elephc_curl_multi_free(multi_id: i64) {
    handles::ffi_guard((), || {
        let mut guard = handles::lock_recover(multis());
        let Some(entry) = guard.remove(&multi_id) else {
            return;
        };
        drop(guard);
        for easy_id in &entry.attached {
            if let Some(easy) = easy_ptr(*easy_id) {
                unsafe { curl_multi_remove_handle(entry.multi, easy) };
            }
        }
        unsafe { curl_multi_cleanup(entry.multi) };
    })
}

/// Records `code` as multi handle `multi_id`'s last `CURLMcode` (for
/// `curl_multi_errno()`) and hands it back, so a caller can `return
/// record_errno(id, code)`.
fn record_errno(multi_id: i64, code: CURLMcode) -> CURLMcode {
    let mut guard = handles::lock_recover(multis());
    if let Some(entry) = guard.get_mut(&multi_id) {
        entry.last_errno = code;
    }
    code
}
