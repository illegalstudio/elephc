//! Purpose:
//! The `curl_mime` builder for `CURLOPT_POSTFIELDS`'s array (multipart-upload) form:
//! the raw libcurl mime bindings, and the small state
//! machine (`new_pending` -> `add_part` -> `set_field`* -> `post`, or `abort` on failure)
//! `crate::abi`'s five `elephc_curl_mime_*` entry points drive to build a
//! `multipart/form-data` body one PHP array item at a time.
//!
//! Called from:
//! - `crate::abi::elephc_curl_mime_new`/`_add_part`/`_part_field`/`_post`/`_abort`.
//! - `crate::handles::EasyEntry::free_mime` (reset/teardown).
//!
//! Key details:
//! - WHY A BUILDER, NOT ONE BIG CALL. PHP hands `CURLOPT_POSTFIELDS` an array of arbitrary
//!   size, each item a scalar, a `CURLFile`, or a `CURLStringFile` with up to four string
//!   fields of its own (name, data/path, type, postname). A single ABI call wide enough to
//!   carry all of that would need either a hand-rolled binary framing for a variable number
//!   of variable-length fields, or more C argument registers than this crate's calling
//!   convention has to spare (`crate::codegen`'s curl lowerings cap out at five). Walking
//!   the array in PHP and driving a builder one field at a time keeps every ABI call to the
//!   same `(handle[, kind], bytes)` shape the rest of this crate already uses for
//!   `curl_setopt()`'s own string/slist setters.
//! - THE PENDING/ATTACHED SPLIT is what makes a failed array walk safe. `pending_mime`
//!   (`EasyEntry`) is under construction and not yet visible to libcurl; `mime` is the one
//!   actually set via `CURLOPT_MIMEPOST`. A PHP-level failure partway through the array
//!   (an unsupported value shape, an embedded NUL in a name, a libcurl-level rejection)
//!   calls [`abort`], which frees ONLY the half-built pending structure — the handle keeps
//!   whatever mime an earlier, successful `curl_setopt()` call already attached, exactly as
//!   a failed `curl_setopt(..., CURLOPT_HTTPHEADER, ...)` leaves the previous header list
//!   alone (`crate::abi::elephc_curl_easy_setopt_slist`).
//! - `FIELD_DATA` IS THE ONE BINARY-SAFE FIELD. `curl_mime_data(part, ptr, len)` takes an
//!   explicit length, so a scalar `CURLOPT_POSTFIELDS` array value or a `CURLStringFile`'s
//!   `$data` survives an embedded NUL intact — matching php-src's own
//!   `curl_mime_data(part, Z_STRVAL_P(value), Z_STRLEN_P(value))`. Every other field
//!   (name, a `CURLFile`'s disk path, the MIME type, the posted filename) goes through
//!   libcurl's NUL-terminated C-string setters, so those fail closed (`false`) on an
//!   embedded NUL rather than silently truncating — the same contract
//!   `crate::abi::elephc_curl_easy_setopt_str` already has for every other string option.
//! - `CURLOPT_MIMEPOST` (10269 = `CURLOPTTYPE_OBJECTPOINT` (10000) + 269, from the pinned
//!   `curl.h`) IS NOT PHP-VISIBLE SURFACE. php-src builds this structure internally
//!   whenever `CURLOPT_POSTFIELDS` is given an array and never exposes the option number as
//!   a userland `CURL_*` constant (confirmed against `scripts/docs/curl_surface.json`: no
//!   `CURLOPT_MIMEPOST` entry). elephc's curl prelude reaches this file the same way, from
//!   `curl_setopt()`'s existing `$option === 10015 && is_array($value)` branch, never
//!   through a PHP-visible option number of its own.
//! - `curl_easy_duphandle` DUPLICATES A HANDLE'S ATTACHED MIME, per libcurl 8.21.0's
//!   `lib/easy.c` `dupset` (already established at `crate::abi::elephc_curl_easy_duphandle`'s
//!   own doc comment), so
//!   `curl_copy_handle()` needs no mime-specific rebuild the way it rebuilds slists — see
//!   that function for the one open question this leaves (the DUPLICATE's own copy of the
//!   structure is never a pointer this crate holds, so it is never a pointer this crate can
//!   `curl_mime_free` either; whatever `curl_easy_cleanup` does with it internally is
//!   between the duplicate handle and libcurl).

use std::ffi::{c_char, c_int, c_void, CString};

use crate::easy::{self, CURLcode, CURL, CURLE_OK};
use crate::handles::EasyEntry;

/// Opaque `curl_mime` context. Never constructed on the Rust side; only ever seen as
/// `*mut CurlMime` returned by [`curl_mime_init`].
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum CurlMime {}

/// Opaque `curl_mimepart` context, owned by the `curl_mime` it was added to
/// ([`curl_mime_addpart`]) and freed as part of that structure — never independently.
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum CurlMimePart {}

/// `CURLOPT_MIMEPOST` (10269). See this module's doc comment for why PHP never names this
/// option directly.
pub(crate) const CURLOPT_MIMEPOST: c_int = 10269;

/// `elephc_curl_mime_part_field`'s `kind`: set the part's field NAME (`curl_mime_name`).
pub(crate) const FIELD_NAME: i32 = 0;
/// `elephc_curl_mime_part_field`'s `kind`: set the part's DATA from memory, binary-safe
/// (`curl_mime_data`). The one field kind that is not NUL-terminated.
pub(crate) const FIELD_DATA: i32 = 1;
/// `elephc_curl_mime_part_field`'s `kind`: set the part's data source to a local file path,
/// read at transfer time (`curl_mime_filedata`).
pub(crate) const FIELD_FILEDATA: i32 = 2;
/// `elephc_curl_mime_part_field`'s `kind`: set the part's `Content-Type` (`curl_mime_type`).
pub(crate) const FIELD_TYPE: i32 = 3;
/// `elephc_curl_mime_part_field`'s `kind`: set the part's remote/posted filename
/// (`curl_mime_filename`).
pub(crate) const FIELD_FILENAME: i32 = 4;

extern "C" {
    /// Creates a mime context tied to `easy`, or null on allocation failure.
    fn curl_mime_init(easy: *mut CURL) -> *mut CurlMime;
    /// Releases a mime handle and every part inside it. Safe on null.
    fn curl_mime_free(mime: *mut CurlMime);
    /// Appends a new, empty part to `mime` and returns a handle to it, or null on
    /// allocation failure.
    fn curl_mime_addpart(mime: *mut CurlMime) -> *mut CurlMimePart;
    /// Sets a mime/form part's field name.
    fn curl_mime_name(part: *mut CurlMimePart, name: *const c_char) -> CURLcode;
    /// Sets a mime part's remote (posted) file name.
    fn curl_mime_filename(part: *mut CurlMimePart, filename: *const c_char) -> CURLcode;
    /// Sets a mime part's `Content-Type`.
    fn curl_mime_type(part: *mut CurlMimePart, mimetype: *const c_char) -> CURLcode;
    /// Sets a mime part's data source from memory, with an explicit (binary-safe) length.
    fn curl_mime_data(part: *mut CurlMimePart, data: *const c_char, datasize: usize) -> CURLcode;
    /// Sets a mime part's data source to a named local file, read at transfer time.
    fn curl_mime_filedata(part: *mut CurlMimePart, filename: *const c_char) -> CURLcode;
}

/// Starts a fresh mime builder for `entry`, discarding any earlier PENDING (never-posted)
/// one — the state a `curl_setopt(..., CURLOPT_POSTFIELDS, $array)` call left behind if it
/// was interrupted (a panic caught by `handles::ffi_guard`) before it could `post`/`abort`.
/// Does NOT touch `entry.mime`, the currently ATTACHED structure from an earlier
/// SUCCESSFUL call — that is only ever replaced by [`post`].
///
/// # Safety
/// `entry.curl` must be a still-live pointer from `easy::init`.
pub(crate) unsafe fn new_pending(entry: &mut EasyEntry) -> bool {
    if let Some(stale) = entry.pending_mime.take() {
        unsafe { curl_mime_free(stale) };
    }
    entry.pending_part = None;
    let mime = unsafe { curl_mime_init(entry.curl) };
    if mime.is_null() {
        return false;
    }
    entry.pending_mime = Some(mime);
    true
}

/// Appends a fresh, empty part to the pending builder and makes it the part every
/// following [`set_field`] writes to. Fails (without touching anything) when
/// [`new_pending`] was never called or libcurl could not allocate the part.
///
/// # Safety
/// `entry.curl` must be a still-live pointer from `easy::init`.
pub(crate) unsafe fn add_part(entry: &mut EasyEntry) -> bool {
    let Some(mime) = entry.pending_mime else {
        return false;
    };
    let part = unsafe { curl_mime_addpart(mime) };
    if part.is_null() {
        return false;
    }
    entry.pending_part = Some(part);
    true
}

/// Sets one field on the current pending part (see the `FIELD_*` constants for `kind`).
/// `false` when there is no current part (`add_part` was never called), `kind` is not one
/// of the five recognized codes, a non-[`FIELD_DATA`] field carries an embedded NUL (which
/// would silently truncate through libcurl's C-string setters), or libcurl itself rejects
/// the value.
///
/// # Safety
/// `entry.curl` must be a still-live pointer from `easy::init` (not read directly by this
/// function, but part of the invariant the caller-held `entry` carries).
pub(crate) unsafe fn set_field(entry: &mut EasyEntry, kind: i32, bytes: &[u8]) -> bool {
    let Some(part) = entry.pending_part else {
        return false;
    };
    let code: CURLcode = match kind {
        FIELD_DATA => {
            // A zero-length value still needs a non-null pointer: libcurl's mime data
            // setter, like `CURLOPT_COPYPOSTFIELDS` (`crate::abi::set_postfields`), reads
            // exactly `datasize` bytes rather than treating null as "no data".
            let ptr = if bytes.is_empty() {
                c"".as_ptr()
            } else {
                bytes.as_ptr() as *const c_char
            };
            unsafe { curl_mime_data(part, ptr, bytes.len()) }
        }
        FIELD_NAME | FIELD_FILEDATA | FIELD_TYPE | FIELD_FILENAME => {
            let Ok(text) = CString::new(bytes) else {
                return false;
            };
            let setter: unsafe extern "C" fn(*mut CurlMimePart, *const c_char) -> CURLcode =
                match kind {
                    FIELD_NAME => curl_mime_name,
                    FIELD_FILEDATA => curl_mime_filedata,
                    FIELD_TYPE => curl_mime_type,
                    FIELD_FILENAME => curl_mime_filename,
                    _ => unreachable!("guarded by the outer match arm"),
                };
            unsafe { setter(part, text.as_ptr()) }
        }
        _ => return false,
    };
    code == CURLE_OK
}

/// Attaches the pending builder to `entry.curl` via `CURLOPT_MIMEPOST`. The PREVIOUSLY
/// attached mime (if any) is freed only AFTER libcurl has accepted the replacement — the
/// same "free the old one only once the new one is live" rule
/// `crate::abi::elephc_curl_easy_setopt_slist` applies to `CURLOPT_HTTPHEADER`, and for the
/// identical reason: a `curl_easy_setopt` failure must leave the handle exactly as it was,
/// never pointing at freed memory.
///
/// On failure (no pending builder, or libcurl refuses the option) the pending builder is
/// freed here and `entry.mime` is left untouched.
///
/// # Safety
/// `entry.curl` must be a still-live pointer from `easy::init`.
pub(crate) unsafe fn post(entry: &mut EasyEntry) -> bool {
    let Some(pending) = entry.pending_mime.take() else {
        return false;
    };
    entry.pending_part = None;
    let code = unsafe { easy::setopt_ptr(entry.curl, CURLOPT_MIMEPOST, pending as *mut c_void) };
    if code != CURLE_OK {
        unsafe { curl_mime_free(pending) };
        return false;
    }
    if let Some(previous) = entry.mime.replace(pending) {
        unsafe { curl_mime_free(previous) };
    }
    true
}

/// Discards the pending builder without attaching it, for a PHP-level array walk that
/// failed partway through (an unsupported value shape, a field this crate or libcurl
/// itself refused). Leaves `entry.mime` — whatever is already attached from an earlier,
/// successful call — untouched. Idempotent: a no-op when there is no pending builder.
pub(crate) fn abort(entry: &mut EasyEntry) {
    if let Some(pending) = entry.pending_mime.take() {
        // SAFETY: `pending` was built by `new_pending`/`add_part`/`set_field` above, is
        // owned solely by this entry, and is taken out of the field as it is freed, so no
        // double free is reachable.
        unsafe { curl_mime_free(pending) };
    }
    entry.pending_part = None;
}

/// Frees whatever mime state `entry` owns — the ATTACHED structure and any half-built
/// PENDING one — and forgets both. Mirrors `EasyEntry::free_slists`; called from
/// `EasyEntry::free_mime` (`curl_reset()`/handle teardown), always AFTER libcurl's own
/// `curl_easy_reset`/`curl_easy_cleanup` has run, matching `free_slists`'s ordering for the
/// same reason: libcurl may still hold a live pointer into the structure until then.
pub(crate) fn free_all(entry: &mut EasyEntry) {
    if let Some(mime) = entry.mime.take() {
        // SAFETY: owned solely by this entry; taken out as it is freed.
        unsafe { curl_mime_free(mime) };
    }
    if let Some(pending) = entry.pending_mime.take() {
        // SAFETY: same as above.
        unsafe { curl_mime_free(pending) };
    }
    entry.pending_part = None;
}
