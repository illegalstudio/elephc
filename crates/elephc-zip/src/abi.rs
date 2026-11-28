//! Purpose:
//! The stable, panic-free C ABI of the `elephc_zip` bridge: handle registry, metadata
//! accessors, mutations, extraction, and supported-method queries over libzip.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_zip"` block in `src/zip_prelude.rs`.
//!
//! Key details:
//! - Handles are positive integers; `0` is never valid and unknown handles fail closed.
//! - Strings may contain NUL: callers pass an explicit byte length and binary results use hex.
//! - Returned strings live in thread-local cells until the next call on the same thread.
//! - `close` only frees libzip's handle on success, so a failed close can still report
//!   `status`/`getStatusString()`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::archive::Archive;
use crate::consts::FL_NOCASE;

/// One entry's metadata captured before `zip_close` removes the live handle.
struct SnapshotEntry {
    /// Current name, as `zip_get_name(za, index, 0)` returns it.
    name: Option<Vec<u8>>,
    /// Original name, as `zip_get_name(za, index, FL_UNCHANGED)` returns it.
    unchanged: Option<Vec<u8>>,
    /// Current stat record, when libzip can produce one.
    stat: Option<SnapshotStat>,
}

/// One numeric stat record captured before close.
struct SnapshotStat {
    /// Entry index.
    index: i64,
    /// CRC-32.
    crc: i64,
    /// Uncompressed size.
    size: i64,
    /// Modification time.
    mtime: i64,
    /// Compressed size.
    comp_size: i64,
    /// Compression method.
    comp_method: i64,
    /// Encryption method.
    encryption_method: i64,
    /// libzip stat flags.
    flags: i64,
}

impl SnapshotStat {
    /// Returns one numeric stat field, using the ABI's selector mapping.
    fn field(&self, field: i64) -> i64 {
        match field {
            0 => self.index,
            1 => self.crc,
            2 => self.size,
            3 => self.mtime,
            4 => self.comp_size,
            5 => self.comp_method,
            6 => self.encryption_method,
            7 => self.flags,
            _ => -1,
        }
    }
}

/// Read-only archive view used by PHP callbacks while libzip is closing the handle.
///
/// `elephc_zip_close` removes the archive from the registry so callbacks can re-enter
/// the bridge for other handles. This snapshot keeps `numFiles`, `statIndex`,
/// `getNameIndex`, `locateName`, `getStatusString`, and the filename/comment readable
/// from inside the callback, as php-src does while `zip_close` still owns the archive.
struct ClosingSnapshot {
    /// Archive path exposed through `ZipArchive::$filename`.
    filename: String,
    /// Archive comment, if any.
    comment: Option<Vec<u8>>,
    /// Per-index names and stats.
    entries: Vec<SnapshotEntry>,
    /// Last status before close started.
    status: (i64, i64),
    /// Human-readable status before close started.
    status_string: String,
    /// PHP's `lastId` property before close started.
    last_id: i64,
}

impl ClosingSnapshot {
    /// Captures the read-visible state of `archive`.
    fn capture(archive: &Archive) -> Self {
        let count = archive.num_entries().max(0);
        let mut entries = Vec::with_capacity(count as usize);
        for index in 0..count {
            let stat = archive.stat(index, 0).ok().map(|stat| SnapshotStat {
                index: stat.index as i64,
                crc: i64::from(stat.crc),
                size: stat.size as i64,
                mtime: stat.mtime,
                comp_size: stat.comp_size as i64,
                comp_method: i64::from(stat.comp_method),
                encryption_method: i64::from(stat.encryption_method),
                flags: i64::from(stat.flags),
            });
            entries.push(SnapshotEntry {
                name: archive.name(index, 0),
                unchanged: archive.name(index, crate::consts::FL_UNCHANGED),
                stat,
            });
        }
        Self {
            filename: archive.path.clone(),
            comment: archive.archive_comment(0),
            entries,
            status: archive.status(),
            status_string: archive.status_string(),
            last_id: archive.last_id,
        }
    }

    /// Returns the number of captured entries.
    fn num_entries(&self) -> i64 {
        self.entries.len() as i64
    }

    /// Returns one captured name, preferring the original when `flags` requests it.
    fn name(&self, index: i64, flags: i64) -> Option<Vec<u8>> {
        let entry = self.entries.get(usize::try_from(index).ok()?)?;
        if flags & crate::consts::FL_UNCHANGED != 0 {
            entry.unchanged.clone()
        } else {
            entry.name.clone()
        }
    }

    /// Locates a captured entry by name, honoring `FL_NOCASE`.
    fn locate(&self, wanted: &[u8], flags: i64) -> Option<i64> {
        let case_insensitive = flags & FL_NOCASE != 0;
        self.entries.iter().position(|entry| {
            entry.name.as_deref().is_some_and(|name| {
                if case_insensitive {
                    name.eq_ignore_ascii_case(wanted)
                } else {
                    name == wanted
                }
            })
        }).map(|index| index as i64)
    }

    /// Returns one numeric stat field for a captured index.
    fn stat_field(&self, index: i64, field: i64) -> i64 {
        let Ok(index) = usize::try_from(index) else {
            return -1;
        };
        self.entries
            .get(index)
            .and_then(|entry| entry.stat.as_ref())
            .map_or(-1, |stat| stat.field(field))
    }
}

thread_local! {
    /// Archives currently inside `zip_close`, indexed by handle.
    static CLOSING_SNAPSHOTS: RefCell<HashMap<i64, ClosingSnapshot>> =
        RefCell::new(HashMap::new());
}

/// Runs `read` against the closing snapshot for `handle`, if one exists.
fn with_closing_snapshot<R>(handle: i64, read: impl FnOnce(&ClosingSnapshot) -> R) -> Option<R> {
    CLOSING_SNAPSHOTS.with(|slot| slot.borrow().get(&handle).map(read))
}

/// Runs `call` against the registered archive, without the closing-snapshot fallback.
fn with_registered_archive<R>(handle: i64, call: impl FnOnce(&Archive) -> R) -> Option<R> {
    let guard = registry().lock().unwrap_or_else(|error| error.into_inner());
    guard.get(&handle).map(call)
}

/// Next handle value; 0 is reserved for "no archive".
static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);

/// The process-wide handle table.
fn registry() -> &'static Mutex<HashMap<i64, Archive>> {
    static REGISTRY: OnceLock<Mutex<HashMap<i64, Archive>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    /// Last text/hex result returned by a string-returning entry point.
    static RESULT: RefCell<CString> = RefCell::new(CString::default());
    /// Whether the last string-returning call produced a real result.
    static RESULT_OK: Cell<bool> = const { Cell::new(false) };
    /// Whether the last stream read found a CRC mismatch.
    static STREAM_CRC_ERROR: Cell<bool> = const { Cell::new(false) };
    /// Whether the last `getFrom*()` payload read failed with a data error.
    ///
    /// This is true when `zip_fread()` fails after a successful open, and also when
    /// libzip reports a CRC/inconsistency while opening the stored entry.
    static GET_READ_FAILED: Cell<bool> = const { Cell::new(false) };
    /// Error code from the most recent failed `open`, consumed by the prelude.
    static OPEN_ERROR: Cell<i64> = const { Cell::new(0) };
}

/// Converts a borrowed pointer plus explicit length into bytes.
unsafe fn bytes(pointer: *const c_char, length: i64) -> Vec<u8> {
    if pointer.is_null() {
        return Vec::new();
    }
    if length >= 0 {
        let slice = std::slice::from_raw_parts(pointer.cast::<u8>(), length as usize);
        return slice.to_vec();
    }
    CStr::from_ptr(pointer).to_bytes().to_vec()
}

/// Stores a hex-encoded result and marks it valid.
fn result_hex(value: &[u8]) -> *const c_char {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len().saturating_mul(2));
    for byte in value {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    result_text(&encoded)
}

/// Stores a text result and marks it valid.
fn result_text(value: &str) -> *const c_char {
    RESULT_OK.with(|flag| flag.set(true));
    RESULT.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = CString::new(value).unwrap_or_default();
        slot.as_ptr()
    })
}

/// Stores an empty invalid result.
fn result_empty() -> *const c_char {
    RESULT_OK.with(|flag| flag.set(false));
    RESULT.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = CString::default();
        slot.as_ptr()
    })
}

/// Runs one registry operation under panic protection.
fn with_archive<R>(handle: i64, fallback: R, call: impl FnOnce(&mut Archive) -> R) -> R {
    catch_unwind(AssertUnwindSafe(|| {
        let mut guard = registry().lock().unwrap_or_else(|error| error.into_inner());
        guard.get_mut(&handle).map(call)
    }))
    .ok()
    .flatten()
    .unwrap_or(fallback)
}

/// Records a bridge-side rejection only when libzip has not already set an error.
fn record_failure(archive: &mut Archive, error: crate::error::ZipError) {
    if archive.status() == (0, 0) {
        archive.fail(error);
    }
}

/// Runs one fallible mutation, recording success or failure on the archive.
fn mutation(handle: i64, call: impl FnOnce(&mut Archive) -> Result<i64, crate::error::ZipError>) -> i64 {
    with_archive(handle, -1, |archive| match call(archive) {
        Ok(value) => {
            archive.succeed();
            value
        }
        Err(error) => {
            record_failure(archive, error);
            -1
        }
    })
}

/// Runs one fallible mutation that has no numeric result.
fn mutation_unit(handle: i64, call: impl FnOnce(&mut Archive) -> Result<(), crate::error::ZipError>) -> i64 {
    mutation(handle, |archive| call(archive).map(|_| 1))
}

/// Opens or creates an archive and returns its handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_open(path: *const c_char, path_len: i64, flags: i64) -> i64 {
    let path = bytes(path, path_len);
    let Ok(path) = String::from_utf8(path) else {
        OPEN_ERROR.with(|slot| slot.set(crate::error::ZipError::Invalid.code()));
        return 0;
    };
    let opened = catch_unwind(AssertUnwindSafe(|| Archive::open(path, flags)))
        .unwrap_or_else(|_| Err(crate::error::ZipError::Internal.code()));
    match opened {
        Ok(archive) => {
            OPEN_ERROR.with(|slot| slot.set(0));
            let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
            registry()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(handle, archive);
            handle
        }
        Err(error) => {
            OPEN_ERROR.with(|slot| slot.set(error));
            0
        }
    }
}

/// Returns the error code left by the most recent failed `open`.
#[no_mangle]
pub extern "C" fn elephc_zip_last_open_error() -> i64 {
    OPEN_ERROR.with(|slot| slot.get())
}

/// Closes an archive and releases its handle on success.
#[no_mangle]
pub extern "C" fn elephc_zip_close(handle: i64) -> i64 {
    // Remove the archive while libzip writes so a PHP progress/cancel callback can
    // re-enter the bridge for other handles without deadlocking on the registry.
    let Some(mut archive) = registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&handle)
    else {
        return -1;
    };
    let snapshot = ClosingSnapshot::capture(&archive);
    CLOSING_SNAPSHOTS.with(|slot| slot.borrow_mut().insert(handle, snapshot));
    let result = catch_unwind(AssertUnwindSafe(|| archive.close()))
        .unwrap_or(Err(crate::error::ZipError::Internal));
    registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(handle, archive);
    match result {
        Ok(()) => 1,
        Err(_) => -1,
    }
}

/// Installs or clears the bridge progress callback for one archive.
#[no_mangle]
pub extern "C" fn elephc_zip_set_progress_callback(
    handle: i64,
    descriptor: *mut c_void,
    adapter: *mut c_void,
    rate: f64,
) -> i64 {
    with_archive(handle, 0, |archive| {
        archive.set_progress_callback(descriptor, adapter, rate);
        1
    })
}

/// Installs or clears the bridge cancel callback for one archive.
#[no_mangle]
pub extern "C" fn elephc_zip_set_cancel_callback(
    handle: i64,
    descriptor: *mut c_void,
    adapter: *mut c_void,
) -> i64 {
    with_archive(handle, 0, |archive| {
        archive.set_cancel_callback(descriptor, adapter);
        1
    })
}

/// Discards uncommitted changes and releases the handle.
#[no_mangle]
pub extern "C" fn elephc_zip_discard(handle: i64) {
    CLOSING_SNAPSHOTS.with(|slot| slot.borrow_mut().remove(&handle));
    let _ = catch_unwind(AssertUnwindSafe(|| {
        registry().lock().unwrap_or_else(|error| error.into_inner()).remove(&handle);
    }));
}

/// Returns the last error code.
#[no_mangle]
pub extern "C" fn elephc_zip_status(handle: i64) -> i64 {
    if let Some(status) = with_registered_archive(handle, |archive| archive.status().0) {
        return status;
    }
    with_closing_snapshot(handle, |snapshot| snapshot.status.0).unwrap_or(8)
}

/// Returns the last OS error number.
#[no_mangle]
pub extern "C" fn elephc_zip_status_sys(handle: i64) -> i64 {
    if let Some(status) = with_registered_archive(handle, |archive| archive.status().1) {
        return status;
    }
    with_closing_snapshot(handle, |snapshot| snapshot.status.1).unwrap_or(0)
}

/// Returns libzip's English status string.
#[no_mangle]
pub extern "C" fn elephc_zip_status_string(handle: i64) -> *const c_char {
    let message = with_registered_archive(handle, |archive| archive.status_string())
        .or_else(|| with_closing_snapshot(handle, |snapshot| snapshot.status_string.clone()))
        .unwrap_or_else(|| "Containing zip archive was closed".to_string());
    result_text(&message)
}

/// Overrides the stored error code for a PHP wrapper that rejected an argument itself.
#[no_mangle]
pub extern "C" fn elephc_zip_set_status(handle: i64, code: i64) {
    with_archive(handle, (), |archive| archive.set_status(code));
}

/// Clears the stored error.
#[no_mangle]
pub extern "C" fn elephc_zip_clear_error(handle: i64) {
    with_archive(handle, (), |archive| archive.clear_error());
}

/// Returns whether the previous string-returning call produced a result.
#[no_mangle]
pub extern "C" fn elephc_zip_result_ok() -> i64 {
    RESULT_OK.with(|flag| i64::from(flag.get()))
}

/// Returns whether open consumed an existing zero-byte file.
#[no_mangle]
pub extern "C" fn elephc_zip_used_empty_file(handle: i64) -> i64 {
    with_archive(handle, 0, |archive| i64::from(archive.empty_file))
}

/// Returns `lastId`.
#[no_mangle]
pub extern "C" fn elephc_zip_last_id(handle: i64) -> i64 {
    if let Some(last_id) = with_closing_snapshot(handle, |snapshot| snapshot.last_id) {
        return last_id;
    }
    with_archive(handle, -1, |archive| archive.last_id)
}

/// Returns the entry count including deleted placeholders.
#[no_mangle]
pub extern "C" fn elephc_zip_num_entries(handle: i64) -> i64 {
    if let Some(count) = with_closing_snapshot(handle, ClosingSnapshot::num_entries) {
        return count;
    }
    with_archive(handle, 0, |archive| archive.num_entries())
}

/// Returns the archive filename.
#[no_mangle]
pub extern "C" fn elephc_zip_filename(handle: i64) -> *const c_char {
    let value = with_closing_snapshot(handle, |snapshot| snapshot.filename.clone())
        .unwrap_or_else(|| with_archive(handle, String::new(), |archive| archive.path.clone()));
    result_text(&value)
}

/// Returns the archive's current comment as hex.
#[no_mangle]
pub extern "C" fn elephc_zip_archive_comment(handle: i64, flags: i64) -> *const c_char {
    let value = with_closing_snapshot(handle, |snapshot| snapshot.comment.clone())
        .unwrap_or_else(|| with_archive(handle, None, |archive| archive.archive_comment(flags)));
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Returns one entry's name as hex, honoring raw/CP437/UTF-8 flags.
#[no_mangle]
pub extern "C" fn elephc_zip_name(handle: i64, index: i64, _raw: i64, flags: i64) -> *const c_char {
    let value = with_closing_snapshot(handle, |snapshot| snapshot.name(index, flags))
        .unwrap_or_else(|| with_archive(handle, None, |archive| archive.name(index, flags)));
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Locates an entry by name, honoring case/directory/unchanged flags.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_locate(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    if let Some(index) = with_closing_snapshot(handle, |snapshot| snapshot.locate(&wanted, flags)) {
        return index.unwrap_or(-1);
    }
    with_archive(handle, -1, |archive| match archive.locate(&wanted, flags) {
        Some(index) => {
            archive.succeed();
            index
        }
        None => -1,
    })
}

/// Returns one numeric stat field for an index.
#[no_mangle]
pub extern "C" fn elephc_zip_stat_index(handle: i64, index: i64, flags: i64, field: i64) -> i64 {
    if let Some(value) = with_closing_snapshot(handle, |snapshot| snapshot.stat_field(index, field)) {
        return value;
    }
    stat_field(handle, field, |archive| archive.stat(index, flags))
}

/// Returns one numeric stat field for a name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_stat_name(handle: i64, name: *const c_char, name_len: i64, flags: i64, field: i64) -> i64 {
    let wanted = bytes(name, name_len);
    if let Some(index) = with_closing_snapshot(handle, |snapshot| snapshot.locate(&wanted, flags)) {
        return index.map_or(-1, |index| {
            with_closing_snapshot(handle, |snapshot| snapshot.stat_field(index, field)).unwrap_or(-1)
        });
    }
    stat_field(handle, field, |archive| archive.stat_name(&wanted, flags))
}

/// Reads one numeric field from an entry stat record.
fn stat_field(
    handle: i64,
    field: i64,
    stat: impl FnOnce(&Archive) -> Result<crate::ffi::zip_stat, crate::error::ZipError>,
) -> i64 {
    with_archive(handle, -1, |archive| match stat(archive) {
        Ok(stat) => {
            archive.succeed();
            match field {
                0 => stat.index as i64,
                1 => i64::from(stat.crc),
                2 => stat.size as i64,
                3 => stat.mtime,
                4 => stat.comp_size as i64,
                5 => i64::from(stat.comp_method),
                6 => i64::from(stat.encryption_method),
                7 => i64::from(stat.flags),
                _ => -1,
            }
        }
        Err(error) => {
            record_failure(archive, error);
            -1
        }
    })
}

/// Returns one entry comment as hex.
#[no_mangle]
pub extern "C" fn elephc_zip_comment_index(handle: i64, index: i64, flags: i64) -> *const c_char {
    // php-src returns "" for an existing entry without a comment; only a missing
    // entry or a closed archive yields false.
    let value = with_archive(handle, None, |archive| {
        archive
            .comment(index, flags)
            .or_else(|| archive.name(index, flags).map(|_| Vec::new()))
    });
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Returns one entry comment by name as hex.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_comment_name(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> *const c_char {
    let wanted = bytes(name, name_len);
    // php-src locates getCommentName() entries with flags 0 and only forwards the
    // requested flags to the comment read itself.
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_comment_index(handle, index, flags),
        None => result_empty(),
    }
}

/// Reads an entry payload as hex, honoring `len`, `FL_COMPRESSED`, and `FL_UNCHANGED`.
#[no_mangle]
pub extern "C" fn elephc_zip_get_index(handle: i64, index: i64, len: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| {
        let result = if len > 0 {
            archive.payload_prefix(index, flags, len as usize)
        } else {
            archive.payload_stream(index, flags)
        };
        match result {
            Ok((mut data, crc_error)) => {
                // php-src's getFrom*() fails the whole read when libzip reports a CRC
                // mismatch, even if some bytes were decoded.
                if crc_error {
                    GET_READ_FAILED.with(|flag| flag.set(true));
                    record_failure(archive, crate::error::ZipError::Crc);
                    return None;
                }
                if len > 0 && (len as usize) < data.len() {
                    data.truncate(len as usize);
                }
                GET_READ_FAILED.with(|flag| flag.set(false));
                archive.succeed();
                Some(data)
            }
            Err(error) => {
                GET_READ_FAILED.with(|flag| flag.set(archive.read_opened()));
                record_failure(archive, error);
                None
            }
        }
    });
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Returns whether the last `getFrom*()` failure carried a data (read) error.
#[no_mangle]
pub extern "C" fn elephc_zip_get_read_failed() -> i64 {
    GET_READ_FAILED.with(|flag| i64::from(flag.get()))
}

/// Returns the entry name a failed `extractTo()` stopped on, as hex.
#[no_mangle]
pub extern "C" fn elephc_zip_extract_failed_entry(handle: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| archive.extract_failure.clone());
    match value {
        Some(name) if !name.is_empty() => result_hex(&name),
        _ => result_empty(),
    }
}

/// Returns a preformatted warning for a non-entry-read `extractTo()` failure.
#[no_mangle]
pub extern "C" fn elephc_zip_extract_warning(handle: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| archive.extract_warning.clone());
    result_text(value.as_deref().unwrap_or(""))
}

/// Reads an entry payload for a stream wrapper as hex, preserving CRC mismatches.
#[no_mangle]
pub extern "C" fn elephc_zip_get_index_stream(handle: i64, index: i64, len: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| match archive.payload_stream(index, flags) {
        Ok((mut data, crc_error)) => {
            STREAM_CRC_ERROR.with(|flag| flag.set(crc_error));
            if len > 0 && (len as usize) < data.len() {
                data.truncate(len as usize);
            }
            archive.succeed();
            Some(data)
        }
        Err(error) => {
            STREAM_CRC_ERROR.with(|flag| flag.set(false));
            record_failure(archive, error);
            None
        }
    });
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Returns whether the last `elephc_zip_get_index_stream` saw a CRC mismatch.
#[no_mangle]
pub extern "C" fn elephc_zip_stream_crc_error() -> i64 {
    STREAM_CRC_ERROR.with(|flag| i64::from(flag.get()))
}

/// Reads an entry payload by name as hex.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_get_name(handle: i64, name: *const c_char, name_len: i64, len: i64, flags: i64) -> *const c_char {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, flags));
    match index {
        Some(index) => elephc_zip_get_index(handle, index, len, flags),
        None => result_empty(),
    }
}

/// Adds an in-memory entry and returns its index.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_from_string(
    handle: i64,
    name: *const c_char,
    name_len: i64,
    data: *const c_char,
    data_len: i64,
    flags: i64,
) -> i64 {
    let name = bytes(name, name_len);
    let payload = bytes(data, data_len);
    mutation(handle, |archive| archive.add_from_string(&name, payload, flags))
}

/// Adds a directory entry and returns its index.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_dir(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let name = bytes(name, name_len);
    mutation(handle, |archive| archive.add_directory(&name, flags))
}

/// Adds a file slice and returns its index; `-2` means the source file is missing.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_file(
    handle: i64,
    path: *const c_char,
    path_len: i64,
    entry: *const c_char,
    entry_len: i64,
    start: i64,
    length: i64,
    flags: i64,
) -> i64 {
    let path = bytes(path, path_len);
    let entry = bytes(entry, entry_len);
    let Ok(path) = String::from_utf8(path) else { return -3 };
    let result = with_archive(handle, -2, |archive| {
        match archive.add_or_replace_file(&path, Some(&entry), start, length, flags, None) {
            Ok(index) => index,
            Err(crate::error::ZipError::Open) => {
                archive.last_id = -1;
                record_failure(archive, crate::error::ZipError::Open);
                -2
            }
            Err(error) => {
                archive.last_id = -1;
                record_failure(archive, error);
                -1
            }
        }
    });
    result
}

/// Replaces an entry's payload from a file slice; `-2` means the source file is missing.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_replace_file(
    handle: i64,
    path: *const c_char,
    path_len: i64,
    index: i64,
    start: i64,
    length: i64,
    flags: i64,
) -> i64 {
    let path = bytes(path, path_len);
    let Ok(path) = String::from_utf8(path) else { return -3 };
    with_archive(handle, -2, |archive| {
        match archive.add_or_replace_file(&path, None, start, length, flags, Some(index)) {
            Ok(_) => 1,
            Err(crate::error::ZipError::Open) => -2,
            Err(error) => {
                record_failure(archive, error);
                -1
            }
        }
    })
}

/// Deletes one entry.
#[no_mangle]
pub extern "C" fn elephc_zip_delete(handle: i64, index: i64) -> i64 {
    mutation_unit(handle, |archive| archive.delete(index))
}

/// Renames one entry.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_rename(handle: i64, index: i64, name: *const c_char, name_len: i64) -> i64 {
    let name = bytes(name, name_len);
    mutation_unit(handle, |archive| archive.rename(index, &name))
}

/// Sets one entry comment.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_comment_index(handle: i64, index: i64, comment: *const c_char, comment_len: i64) -> i64 {
    let comment = bytes(comment, comment_len);
    mutation_unit(handle, |archive| archive.set_comment(index, &comment))
}

/// Sets one entry comment by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_comment_name(
    handle: i64,
    name: *const c_char,
    name_len: i64,
    comment: *const c_char,
    comment_len: i64,
) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_set_comment_index(handle, index, comment, comment_len),
        None => -1,
    }
}

/// Sets one entry mtime.
#[no_mangle]
pub extern "C" fn elephc_zip_set_mtime_index(handle: i64, index: i64, timestamp: i64, flags: i64) -> i64 {
    mutation_unit(handle, |archive| archive.set_mtime(index, timestamp, flags))
}

/// Sets one entry mtime by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_mtime_name(handle: i64, name: *const c_char, name_len: i64, timestamp: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_set_mtime_index(handle, index, timestamp, flags),
        None => -1,
    }
}

/// Sets one entry's compression method.
#[no_mangle]
pub extern "C" fn elephc_zip_set_compression_index(handle: i64, index: i64, method: i64, compflags: i64) -> i64 {
    mutation_unit(handle, |archive| archive.set_compression(index, method, compflags))
}

/// Sets one entry's compression method by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_compression_name(handle: i64, name: *const c_char, name_len: i64, method: i64, compflags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_set_compression_index(handle, index, method, compflags),
        None => -1,
    }
}

/// Sets one entry's encryption method.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_encryption_index(
    handle: i64,
    index: i64,
    method: i64,
    password: *const c_char,
    password_len: i64,
) -> i64 {
    let password = if password_len < 0 { None } else { Some(bytes(password, password_len)) };
    mutation_unit(handle, |archive| archive.set_encryption(index, method, password.as_deref()))
}

/// Sets one entry's encryption method by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_encryption_name(
    handle: i64,
    name: *const c_char,
    name_len: i64,
    method: i64,
    password: *const c_char,
    password_len: i64,
) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_set_encryption_index(handle, index, method, password, password_len),
        None => -1,
    }
}

/// Sets one entry's external attributes.
#[no_mangle]
pub extern "C" fn elephc_zip_set_external_attributes_index(handle: i64, index: i64, opsys: i64, attr: i64, flags: i64) -> i64 {
    mutation_unit(handle, |archive| archive.set_external_attributes(index, opsys, attr as u32, flags))
}

/// Sets one entry's external attributes by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_external_attributes_name(
    handle: i64,
    name: *const c_char,
    name_len: i64,
    opsys: i64,
    attr: i64,
    flags: i64,
) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_set_external_attributes_index(handle, index, opsys, attr, flags),
        None => -1,
    }
}

/// Returns one entry's `OPSYS_*` value.
#[no_mangle]
pub extern "C" fn elephc_zip_external_opsys(handle: i64, index: i64, flags: i64) -> i64 {
    with_archive(handle, -1, |archive| match archive.external_attributes(index, flags) {
        Ok((opsys, _)) => {
            archive.succeed();
            opsys
        }
        Err(error) => {
            record_failure(archive, error);
            -1
        }
    })
}

/// Returns one entry's external attribute bit field.
#[no_mangle]
pub extern "C" fn elephc_zip_external_attr(handle: i64, index: i64, flags: i64) -> i64 {
    with_archive(handle, -1, |archive| match archive.external_attributes(index, flags) {
        Ok((_, attr)) => {
            archive.succeed();
            i64::from(attr)
        }
        Err(error) => {
            record_failure(archive, error);
            -1
        }
    })
}

/// Returns the `OPSYS_*` value for an entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_external_opsys_name(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_external_opsys(handle, index, flags),
        None => -1,
    }
}

/// Returns the external attribute bit field for an entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_external_attr_name(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_external_attr(handle, index, flags),
        None => -1,
    }
}

/// Sets the archive comment.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_archive_comment(handle: i64, comment: *const c_char, comment_len: i64) -> i64 {
    let comment = bytes(comment, comment_len);
    mutation_unit(handle, |archive| archive.set_archive_comment(&comment))
}

/// Sets the archive default password.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_password(handle: i64, password: *const c_char, password_len: i64) -> i64 {
    let password = bytes(password, password_len);
    mutation_unit(handle, |archive| archive.set_password(&password))
}

/// Gets one archive flag.
#[no_mangle]
pub extern "C" fn elephc_zip_archive_flag(handle: i64, flag: i64, flags: i64) -> i64 {
    with_archive(handle, 0, |archive| archive.archive_flag(flag, flags))
}

/// Sets one archive flag.
#[no_mangle]
pub extern "C" fn elephc_zip_set_archive_flag(handle: i64, flag: i64, value: i64) -> i64 {
    mutation_unit(handle, |archive| archive.set_archive_flag(flag, value))
}

/// Reverts all changes.
#[no_mangle]
pub extern "C" fn elephc_zip_unchange_all(handle: i64) -> i64 {
    mutation_unit(handle, |archive| archive.unchange_all())
}

/// Reverts one entry.
#[no_mangle]
pub extern "C" fn elephc_zip_unchange_index(handle: i64, index: i64) -> i64 {
    mutation_unit(handle, |archive| archive.unchange_index(index))
}

/// Reverts one entry by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_unchange_name(handle: i64, name: *const c_char, name_len: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index {
        Some(index) => elephc_zip_unchange_index(handle, index),
        None => -1,
    }
}

/// Reverts the archive comment and flags.
#[no_mangle]
pub extern "C" fn elephc_zip_unchange_archive(handle: i64) -> i64 {
    mutation_unit(handle, |archive| archive.unchange_archive())
}

/// Extracts selected entries into a destination directory.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_extract(
    handle: i64,
    destination: *const c_char,
    destination_len: i64,
    selection: *const c_char,
    selection_len: i64,
) -> i64 {
    let Ok(destination) = String::from_utf8(bytes(destination, destination_len)) else {
        return 0;
    };
    let selection = crate::extract::parse_selection(&bytes(selection, selection_len));
    mutation_unit(handle, |archive| {
        crate::extract::extract_to(archive, &destination, selection.as_deref())
    })
}

/// Matches a PCRE-style `/pattern/flags` against a subject using the shared regex engine.
///
/// Returns 1 on a match, 0 on no match, and -1 for an invalid expression. The prelude
/// owns PHP's warning strings; this entry point only answers the match question.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_pattern_match(
    pattern: *const c_char,
    pattern_len: i64,
    subject: *const c_char,
    subject_len: i64,
) -> i64 {
    let pattern = bytes(pattern, pattern_len);
    let subject = bytes(subject, subject_len);
    let Some((body, flags)) = pcre_body(&pattern) else { return -1 };
    let Ok(body) = String::from_utf8(body) else { return -1 };
    let mut expression = String::new();
    let mut modifiers = String::new();
    let mut unicode = false;
    let mut anchored = false;
    for flag in flags {
        match flag {
            b'i' | b'm' | b's' | b'x' => modifiers.push(char::from(flag)),
            b'u' => unicode = true,
            b'A' => anchored = true,
            // `U` (ungreedy) has no Rust-regex equivalent; treating it as greedy would
            // silently change which names match, so the caller sees PHP's invalid-expression
            // path instead. `D`/`S`/`X`/`J` do not change which subjects match.
            b'U' => return -1,
            b'D' | b'S' | b'X' | b'J' => {}
            _ => return -1,
        }
    }
    if anchored {
        expression.push_str("\\A(?:");
    }
    if !modifiers.is_empty() {
        expression.push_str("(?");
        expression.push_str(&modifiers);
        expression.push(')');
    }
    expression.push_str(&body);
    if anchored {
        expression.push(')');
    }
    match regex::bytes::RegexBuilder::new(&expression)
        .unicode(unicode)
        .build()
    {
        Ok(regex) => i64::from(regex.is_match(&subject)),
        Err(_) => -1,
    }
}

/// Extracts the expression between the first and last delimiter of a PCRE pattern.
fn pcre_body(pattern: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    if pattern.len() < 3 {
        return None;
    }
    let delimiter = pattern[0];
    let mut end = None;
    let mut escaped = false;
    for (index, &byte) in pattern.iter().enumerate().skip(1) {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == delimiter {
            end = Some(index);
            break;
        }
    }
    let end = end?;
    if end == 0 {
        return None;
    }
    Some((pattern[1..end].to_vec(), pattern[end + 1..].to_vec()))
}

/// Reports whether a compression method is available.
#[no_mangle]
pub extern "C" fn elephc_zip_compression_supported(method: i64, encode: i64) -> i64 {
    i64::from(unsafe { crate::ffi::zip_compression_method_supported(method as i32, i64::from(encode != 0) as i32) })
}

/// Reports whether an encryption method is available.
#[no_mangle]
pub extern "C" fn elephc_zip_encryption_supported(method: i64, encode: i64) -> i64 {
    i64::from(unsafe { crate::ffi::zip_encryption_method_supported(method as u16, i64::from(encode != 0) as i32) })
}

/// Returns the linked libzip version string.
#[no_mangle]
pub extern "C" fn elephc_zip_version() -> *const c_char {
    let pointer = unsafe { crate::ffi::zip_libzip_version() };
    if pointer.is_null() {
        return result_text("1.11.4");
    }
    let version = unsafe { CStr::from_ptr(pointer) }.to_string_lossy();
    result_text(&version)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    /// Verifies the hex encoder emits lowercase nibbles for arbitrary bytes.
    #[test]
    fn result_hex_encodes_arbitrary_bytes() {
        let pointer = result_hex(&[0x00, 0xab, 0xff]);
        let text = unsafe { CStr::from_ptr(pointer) }.to_str().expect("utf-8 hex");
        assert_eq!(text, "00abff");
    }

    /// Verifies addPattern matches non-UTF-8 names byte-wise and honours the `A` modifier.
    #[test]
    fn pattern_match_is_byte_oriented_and_anchored() {
        let plain = CString::new("/foo/").unwrap();
        let anchored = CString::new("/foo/A").unwrap();
        let subject = [0xff, b'f', b'o', b'o'];
        let offset_subject = b"barfoo";
        unsafe {
            assert_eq!(elephc_zip_pattern_match(plain.as_ptr(), 5, subject.as_ptr().cast(), 4), 1);
            assert_eq!(elephc_zip_pattern_match(anchored.as_ptr(), 6, subject.as_ptr().cast(), 4), 0);
            assert_eq!(elephc_zip_pattern_match(anchored.as_ptr(), 6, offset_subject.as_ptr().cast(), 6), 0);
            assert_eq!(elephc_zip_pattern_match(plain.as_ptr(), 5, offset_subject.as_ptr().cast(), 6), 1);
            let trailing = CString::new("#foo#i#x").unwrap();
            assert_eq!(elephc_zip_pattern_match(trailing.as_ptr(), 9, b"foo".as_ptr().cast(), 3), -1);
        }
    }
}
