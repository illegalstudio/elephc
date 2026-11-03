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
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::archive::Archive;

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
    let encoded = value.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
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
        .unwrap_or_else(|_| Err(crate::error::ZipError::Truncated.code()));
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
    with_archive(handle, -1, |archive| match archive.close() {
        Ok(()) => 1,
        Err(_) => -1,
    })
}

/// Discards uncommitted changes and releases the handle.
#[no_mangle]
pub extern "C" fn elephc_zip_discard(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        registry().lock().unwrap_or_else(|error| error.into_inner()).remove(&handle);
    }));
}

/// Returns the last error code.
#[no_mangle]
pub extern "C" fn elephc_zip_status(handle: i64) -> i64 {
    with_archive(handle, 8, |archive| archive.status().0)
}

/// Returns the last OS error number.
#[no_mangle]
pub extern "C" fn elephc_zip_status_sys(handle: i64) -> i64 {
    with_archive(handle, 0, |archive| archive.status().1)
}

/// Returns libzip's English status string.
#[no_mangle]
pub extern "C" fn elephc_zip_status_string(handle: i64) -> *const c_char {
    let message = with_archive(handle, "Containing zip archive was closed".to_string(), |archive| {
        archive.status_string()
    });
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
    with_archive(handle, -1, |archive| archive.last_id)
}

/// Returns the entry count including deleted placeholders.
#[no_mangle]
pub extern "C" fn elephc_zip_num_entries(handle: i64) -> i64 {
    with_archive(handle, 0, |archive| archive.num_entries())
}

/// Returns the archive filename.
#[no_mangle]
pub extern "C" fn elephc_zip_filename(handle: i64) -> *const c_char {
    let value = with_archive(handle, String::new(), |archive| archive.path.clone());
    result_text(&value)
}

/// Returns the archive's current comment as hex.
#[no_mangle]
pub extern "C" fn elephc_zip_archive_comment(handle: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| archive.archive_comment(flags));
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Returns one entry's name as hex, honoring raw/CP437/UTF-8 flags.
#[no_mangle]
pub extern "C" fn elephc_zip_name(handle: i64, index: i64, _raw: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| archive.name(index, flags));
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Locates an entry by name, honoring case/directory/unchanged flags.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_locate(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
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
    stat_field(handle, field, |archive| archive.stat(index, flags))
}

/// Returns one numeric stat field for a name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_stat_name(handle: i64, name: *const c_char, name_len: i64, flags: i64, field: i64) -> i64 {
    let wanted = bytes(name, name_len);
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
    let value = with_archive(handle, None, |archive| archive.comment(index, flags));
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
}

/// Returns one entry comment by name as hex.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_comment_name(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> *const c_char {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, flags));
    match index {
        Some(index) => elephc_zip_comment_index(handle, index, flags),
        None => result_empty(),
    }
}

/// Reads an entry payload as hex, honoring `len`, `FL_COMPRESSED`, and `FL_UNCHANGED`.
#[no_mangle]
pub extern "C" fn elephc_zip_get_index(handle: i64, index: i64, len: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| match archive.payload(index, flags) {
        Ok(mut data) => {
            if len > 0 && (len as usize) < data.len() {
                data.truncate(len as usize);
            }
            archive.succeed();
            Some(data)
        }
        Err(error) => {
            record_failure(archive, error);
            None
        }
    });
    match value {
        Some(value) => result_hex(&value),
        None => result_empty(),
    }
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
    let destination = String::from_utf8_lossy(&bytes(destination, destination_len)).into_owned();
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
    let Ok(subject) = String::from_utf8(subject) else { return 0 };
    let mut expression = String::new();
    let mut modifiers = String::new();
    for flag in flags {
        match flag {
            b'i' | b'm' | b's' | b'x' => modifiers.push(char::from(flag)),
            b'u' | b'U' | b'A' | b'D' | b'S' | b'X' | b'J' => {}
            _ => return -1,
        }
    }
    if !modifiers.is_empty() {
        expression.push_str("(?");
        expression.push_str(&modifiers);
        expression.push(')');
    }
    expression.push_str(&body);
    match regex::Regex::new(&expression) {
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
    let end = pattern.iter().rposition(|&byte| byte == delimiter)?;
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
