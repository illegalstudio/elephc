//! Purpose:
//! The stable, panic-free C ABI of the `elephc_zip` bridge: handle registry, metadata
//! accessors, mutations, extraction, and supported-method queries.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_zip"` block in `src/zip_prelude.rs`.
//!
//! Key details:
//! - Handles are positive integers; `0` is never valid and unknown handles fail closed.
//! - Strings may contain NUL: callers pass an explicit byte length and binary results use hex.
//! - Returned strings live in thread-local cells until the next call on the same thread.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::consts::*;
use crate::error::ZipError;
use crate::model::Archive;

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

/// Stores a UTF-8/text result and marks it valid.
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
    })).ok().flatten().unwrap_or(fallback)
}

/// Runs one fallible mutation, recording success or failure on the archive.
fn mutation(handle: i64, call: impl FnOnce(&mut Archive) -> Result<i64, ZipError>) -> i64 {
    with_archive(handle, -1, |archive| match call(archive) {
        Ok(value) => { archive.success(); value }
        Err(error) => { archive.fail(error); -1 }
    })
}

/// Runs one fallible mutation that has no numeric result.
fn mutation_unit(handle: i64, call: impl FnOnce(&mut Archive) -> Result<(), ZipError>) -> i64 {
    mutation(handle, |archive| call(archive).map(|_| 1))
}

/// Opens or creates an archive and returns its handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_open(path: *const c_char, path_len: i64, flags: i64) -> i64 {
    let path = bytes(path, path_len);
    let Ok(path) = String::from_utf8(path) else {
        OPEN_ERROR.with(|slot| slot.set(ZipError::Invalid.code()));
        return 0;
    };
    let opened = catch_unwind(AssertUnwindSafe(|| Archive::open(path, flags)))
        .unwrap_or_else(|_| Err(ZipError::Truncated));
    match opened {
        Ok(archive) => {
            OPEN_ERROR.with(|slot| slot.set(0));
            let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
            registry().lock().unwrap_or_else(|error| error.into_inner()).insert(handle, archive);
            handle
        }
        Err(error) => {
            OPEN_ERROR.with(|slot| slot.set(error.code()));
            0
        }
    }
}

/// Returns the error code left by the most recent failed `open`.
#[no_mangle]
pub extern "C" fn elephc_zip_last_open_error() -> i64 {
    OPEN_ERROR.with(|slot| slot.get())
}

/// Closes an archive and releases its handle.
#[no_mangle]
pub extern "C" fn elephc_zip_close(handle: i64) -> i64 {
    mutation_unit(handle, |archive| archive.close())
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
    with_archive(handle, 8, |archive| archive.status.code())
}

/// Returns the last OS error number.
#[no_mangle]
pub extern "C" fn elephc_zip_status_sys(handle: i64) -> i64 {
    with_archive(handle, 0, |archive| archive.status_sys)
}

/// Returns libzip's English status string.
#[no_mangle]
pub extern "C" fn elephc_zip_status_string(handle: i64) -> *const c_char {
    let message = with_archive(handle, "Containing zip archive was closed", |archive| archive.status.message());
    result_text(message)
}

/// Overrides the stored error code for a PHP wrapper that rejected an argument itself.
#[no_mangle]
pub extern "C" fn elephc_zip_set_status(handle: i64, code: i64) {
    with_archive(handle, (), |archive| {
        archive.status = crate::error::ZipError::from_code(code);
        archive.status_sys = 0;
    });
}

/// Clears the stored error.
#[no_mangle]
pub extern "C" fn elephc_zip_clear_error(handle: i64) {
    with_archive(handle, (), |archive| archive.success());
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
    with_archive(handle, 0, |archive| archive.num_entries() as i64)
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
    let value = with_archive(handle, None, |archive| {
        Some(if flags & FL_UNCHANGED != 0 { archive.original_comment.clone() } else { archive.comment.clone() })
    });
    match value { Some(value) => result_hex(&value), None => result_empty() }
}

/// Returns one entry's name as hex, honoring raw/CP437/UTF-8 flags.
#[no_mangle]
pub extern "C" fn elephc_zip_name(handle: i64, index: i64, raw: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| {
        let unchanged = flags & FL_UNCHANGED != 0;
        let entry = if unchanged { archive.original.get(index as usize) } else { archive.entries.get(index as usize) }
            .filter(|entry| !entry.deleted)?;
        let bytes = if raw != 0 || flags & FL_ENC_RAW != 0 {
            entry.name_raw.clone()
        } else if flags & FL_ENC_CP437 != 0 {
            entry.name_raw.iter().map(|&byte| cp437(byte)).collect::<String>().into_bytes()
        } else if flags & FL_ENC_UTF_8 != 0 {
            entry.name_raw.clone()
        } else {
            entry.name.clone()
        };
        Some(bytes)
    });
    match value { Some(value) => result_hex(&value), None => result_empty() }
}

/// Returns the default decoded name bytes for a raw byte sequence.
fn cp437(byte: u8) -> char {
    if byte < 0x80 { return char::from(byte); }
    const TABLE: &[char] = &[
        'Ç','ü','é','â','ä','à','å','ç','ê','ë','è','ï','î','ì','Ä','Å','É','æ','Æ','ô','ö','ò','û','ù','ÿ','Ö','Ü','¢','£','¥','₧','ƒ',
        'á','í','ó','ú','ñ','Ñ','ª','º','¿','⌐','¬','½','¼','¡','«','»','░','▒','▓','│','┤','╡','╢','╖','╕','╣','║','╗','╝','╜','╛','┐',
        '└','┴','┬','├','─','┼','╞','╟','╚','╔','╩','╦','╠','═','╬','╧','╨','╤','╥','╙','╘','╒','╓','╫','╪','┘','┌','█','▄','▌','▐','▀',
        'α','ß','Γ','π','Σ','σ','µ','τ','Φ','Θ','Ω','δ','∞','φ','ε','∩','≡','±','≥','≤','⌠','⌡','÷','≈','°','∙','·','√','ⁿ','²','■','\u{a0}',
    ];
    TABLE[(byte - 0x80) as usize]
}

/// Locates an entry by name, honoring case/directory/unchanged flags.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_locate(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    with_archive(handle, -1, |archive| match archive.locate(&wanted, flags) {
        Some(index) => { archive.success(); index as i64 }
        None => { archive.fail(ZipError::NoEnt); -1 }
    })
}

/// Returns a numeric stat field for an index.
#[no_mangle]
pub extern "C" fn elephc_zip_stat_index(handle: i64, index: i64, flags: i64, field: i64) -> i64 {
    with_archive(handle, -1, |archive| match stat_field(archive, index as usize, flags, field) {
        Ok(value) => { archive.success(); value }
        Err(error) => { archive.fail(error); -1 }
    })
}

/// Returns a numeric stat field for a name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_stat_name(handle: i64, name: *const c_char, name_len: i64, flags: i64, field: i64) -> i64 {
    let wanted = bytes(name, name_len);
    with_archive(handle, -1, |archive| {
        let Some(index) = archive.locate(&wanted, flags) else { archive.fail(ZipError::NoEnt); return -1 };
        match stat_field(archive, index, flags, field) {
            Ok(value) => { archive.success(); value }
            Err(error) => { archive.fail(error); -1 }
        }
    })
}

/// Reads one numeric field from an entry.
fn stat_field(archive: &Archive, index: usize, flags: i64, field: i64) -> Result<i64, ZipError> {
    let entry = if flags & FL_UNCHANGED != 0 { archive.original.get(index) } else { archive.entries.get(index) }
        .filter(|entry| !entry.deleted)
        .ok_or(ZipError::NoEnt)?;
    Ok(match field {
        0 => index as i64,
        1 => i64::from(entry.crc),
        2 => entry.size as i64,
        3 => entry.mtime,
        4 => entry.comp_size as i64,
        5 => entry.method,
        6 => entry.encryption,
        7 => i64::from(entry.flags),
        _ => return Err(ZipError::Invalid),
    })
}

/// Returns one entry comment as hex.
#[no_mangle]
pub extern "C" fn elephc_zip_comment_index(handle: i64, index: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| {
        if flags & FL_UNCHANGED != 0 {
            archive.original.get(index as usize).map(|entry| entry.comment.clone())
        } else {
            archive.entries.get(index as usize).filter(|entry| !entry.deleted).map(|entry| entry.comment.clone())
        }
    });
    match value { Some(value) => result_hex(&value), None => result_empty() }
}

/// Returns one entry comment by name as hex.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_comment_name(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> *const c_char {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, flags));
    match index { Some(index) => elephc_zip_comment_index(handle, index as i64, flags), None => result_empty() }
}

/// Reads an entry payload as hex, honoring `len`, `FL_COMPRESSED`, and `FL_UNCHANGED`.
#[no_mangle]
pub extern "C" fn elephc_zip_get_index(handle: i64, index: i64, len: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| {
        let result = if flags & FL_COMPRESSED != 0 {
            archive.compressed_payload(index as usize, flags)
        } else {
            archive.payload(index as usize, flags, None)
        };
        match result {
            Ok(mut data) => {
                if len > 0 && (len as usize) < data.len() { data.truncate(len as usize); }
                archive.success();
                Some(data)
            }
            Err(error) => { archive.fail(error); None }
        }
    });
    match value { Some(value) => result_hex(&value), None => result_empty() }
}

/// Reads an entry payload for a stream wrapper as hex, preserving CRC mismatches.
#[no_mangle]
pub extern "C" fn elephc_zip_get_index_stream(handle: i64, index: i64, len: i64, flags: i64) -> *const c_char {
    let value = with_archive(handle, None, |archive| {
        match archive.payload_stream(index as usize, flags, None) {
            Ok((mut data, crc_error)) => {
                STREAM_CRC_ERROR.with(|flag| flag.set(crc_error));
                if len > 0 && (len as usize) < data.len() { data.truncate(len as usize); }
                archive.success();
                Some(data)
            }
            Err(error) => {
                STREAM_CRC_ERROR.with(|flag| flag.set(false));
                archive.fail(error);
                None
            }
        }
    });
    match value { Some(value) => result_hex(&value), None => result_empty() }
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
    match index { Some(index) => elephc_zip_get_index(handle, index as i64, len, flags), None => result_empty() }
}

/// Adds an in-memory entry and returns its index.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_from_string(
    handle: i64, name: *const c_char, name_len: i64, data: *const c_char, data_len: i64, flags: i64,
) -> i64 {
    let name = bytes(name, name_len);
    let payload = bytes(data, data_len);
    mutation(handle, |archive| archive.add_from_string(&name, payload, flags).map(|index| index as i64))
}

/// Adds a directory entry and returns its index.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_dir(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let name = bytes(name, name_len);
    mutation(handle, |archive| archive.add_directory(&name, flags).map(|index| index as i64))
}

/// Adds a file slice and returns its index; `-2` means the source file is missing.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_file(
    handle: i64, path: *const c_char, path_len: i64, entry: *const c_char, entry_len: i64,
    start: i64, length: i64, flags: i64,
) -> i64 {
    let path = bytes(path, path_len);
    let entry = bytes(entry, entry_len);
    let Ok(path) = String::from_utf8(path) else { return -3 };
    let Ok(data) = std::fs::read(&path) else {
        with_archive(handle, (), |archive| { archive.last_id = -1; });
        return -2;
    };
    let Some(slice) = slice_data(&data, start, length) else { return invalid_parameter(handle) };
    let name = if entry.is_empty() { path.as_bytes().to_vec() } else { entry };
    let remember = (flags & 0x4000_0000) == 0;
    let source = remember.then(|| path.clone());
    mutation(handle, |archive| archive.add_from_path(&name, slice, flags, source).map(|index| index as i64))
}

/// Replaces an entry's payload from a file slice; `-2` means the source file is missing.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_replace_file(
    handle: i64, path: *const c_char, path_len: i64, index: i64, start: i64, length: i64, _flags: i64,
) -> i64 {
    let path = bytes(path, path_len);
    let Ok(path) = String::from_utf8(path) else { return -3 };
    let Ok(data) = std::fs::read(&path) else {
        with_archive(handle, (), |archive| { archive.last_id = -1; });
        return -2;
    };
    let Some(slice) = slice_data(&data, start, length) else { return invalid_parameter(handle) };
    mutation(handle, |archive| archive.replace_entry(index as usize, slice).map(|value| value as i64))
}

/// Returns a file-data slice for `addFile`/`replaceFile` range arguments.
fn slice_data(data: &[u8], start: i64, length: i64) -> Option<Vec<u8>> {
    if start < 0 { return None; }
    let start = start as usize;
    if start > data.len() { return None; }
    let end = match length {
        LENGTH_TO_END | LENGTH_UNCHECKED => data.len(),
        value if value < 0 => return None,
        value => start.checked_add(value as usize)?.min(data.len()),
    };
    if length > 0 && end != start + length as usize { return None; }
    Some(data[start..end].to_vec())
}

/// Records `ER_INVAL` for a malformed range argument and answers the failure sentinel.
fn invalid_parameter(handle: i64) -> i64 {
    with_archive(handle, -1, |archive| {
        archive.fail(ZipError::Invalid);
        -1
    })
}

/// Deletes one entry.
#[no_mangle]
pub extern "C" fn elephc_zip_delete(handle: i64, index: i64) -> i64 {
    mutation(handle, |archive| archive.delete(index as usize).map(|_| 1))
}

/// Renames one entry.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_rename(handle: i64, index: i64, name: *const c_char, name_len: i64) -> i64 {
    let name = bytes(name, name_len);
    mutation(handle, |archive| archive.rename(index as usize, &name).map(|_| 1))
}

/// Sets one entry comment.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_comment_index(handle: i64, index: i64, comment: *const c_char, comment_len: i64) -> i64 {
    let comment = bytes(comment, comment_len);
    mutation(handle, |archive| archive.set_comment(index as usize, &comment).map(|_| 1))
}

/// Sets one entry comment by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_comment_name(handle: i64, name: *const c_char, name_len: i64, comment: *const c_char, comment_len: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_set_comment_index(handle, index as i64, comment, comment_len), None => -1 }
}

/// Sets one entry mtime.
#[no_mangle]
pub extern "C" fn elephc_zip_set_mtime_index(handle: i64, index: i64, timestamp: i64, _flags: i64) -> i64 {
    mutation(handle, |archive| archive.set_mtime(index as usize, timestamp).map(|_| 1))
}

/// Sets one entry mtime by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_mtime_name(handle: i64, name: *const c_char, name_len: i64, timestamp: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_set_mtime_index(handle, index as i64, timestamp, flags), None => -1 }
}

/// Sets one entry's compression method.
#[no_mangle]
pub extern "C" fn elephc_zip_set_compression_index(handle: i64, index: i64, method: i64, _compflags: i64) -> i64 {
    mutation(handle, |archive| archive.set_compression(index as usize, method).map(|_| 1))
}

/// Sets one entry's compression method by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_compression_name(handle: i64, name: *const c_char, name_len: i64, method: i64, compflags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_set_compression_index(handle, index as i64, method, compflags), None => -1 }
}

/// Sets one entry's encryption method.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_encryption_index(handle: i64, index: i64, method: i64, password: *const c_char, password_len: i64) -> i64 {
    let password = if password_len < 0 { None } else { Some(bytes(password, password_len)) };
    mutation(handle, |archive| archive.set_encryption(index as usize, method, password).map(|_| 1))
}

/// Sets one entry's encryption method by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_encryption_name(handle: i64, name: *const c_char, name_len: i64, method: i64, password: *const c_char, password_len: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_set_encryption_index(handle, index as i64, method, password, password_len), None => -1 }
}

/// Sets one entry's external attributes.
#[no_mangle]
pub extern "C" fn elephc_zip_set_external_attributes_index(handle: i64, index: i64, opsys: i64, attr: i64, _flags: i64) -> i64 {
    mutation(handle, |archive| archive.set_external_attributes(index as usize, opsys, attr as u32).map(|_| 1))
}

/// Sets one entry's external attributes by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_external_attributes_name(handle: i64, name: *const c_char, name_len: i64, opsys: i64, attr: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_set_external_attributes_index(handle, index as i64, opsys, attr, flags), None => -1 }
}

/// Returns one entry's `OPSYS_*` value.
#[no_mangle]
pub extern "C" fn elephc_zip_external_opsys(handle: i64, index: i64, flags: i64) -> i64 {
    with_archive(handle, -1, |archive| match archive.external_attributes(index as usize, flags) {
        Ok((opsys, _)) => { archive.success(); opsys }
        Err(error) => { archive.fail(error); -1 }
    })
}

/// Returns one entry's external attribute bit field.
#[no_mangle]
pub extern "C" fn elephc_zip_external_attr(handle: i64, index: i64, flags: i64) -> i64 {
    with_archive(handle, -1, |archive| match archive.external_attributes(index as usize, flags) {
        Ok((_, attr)) => { archive.success(); i64::from(attr) }
        Err(error) => { archive.fail(error); -1 }
    })
}

/// Returns the `OPSYS_*` value for an entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_external_opsys_name(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_external_opsys(handle, index as i64, flags), None => -1 }
}

/// Returns the external attribute bit field for an entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_external_attr_name(handle: i64, name: *const c_char, name_len: i64, flags: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_external_attr(handle, index as i64, flags), None => -1 }
}

/// Sets the archive comment.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_archive_comment(handle: i64, comment: *const c_char, comment_len: i64) -> i64 {
    let comment = bytes(comment, comment_len);
    mutation(handle, |archive| archive.set_archive_comment(&comment).map(|_| 1))
}

/// Sets the archive default password.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_password(handle: i64, password: *const c_char, password_len: i64) -> i64 {
    let password = bytes(password, password_len);
    mutation(handle, |archive| {
        archive.password = Some(password);
        archive.success();
        Ok(1)
    })
}

/// Gets one archive flag.
#[no_mangle]
pub extern "C" fn elephc_zip_archive_flag(handle: i64, flag: i64, _flags: i64) -> i64 {
    with_archive(handle, 0, |archive| match flag {
        AFL_RDONLY => i64::from(archive.archive_flags & AFL_RDONLY != 0),
        AFL_IS_TORRENTZIP => i64::from(archive.archive_flags & AFL_IS_TORRENTZIP != 0),
        AFL_WANT_TORRENTZIP => i64::from(archive.archive_flags & AFL_WANT_TORRENTZIP != 0),
        AFL_KEEP_EMPTY => i64::from(archive.archive_flags & AFL_KEEP_EMPTY != 0),
        _ => 0,
    })
}

/// Sets one archive flag.
#[no_mangle]
pub extern "C" fn elephc_zip_set_archive_flag(handle: i64, flag: i64, value: i64) -> i64 {
    mutation(handle, |archive| archive.set_archive_flag(flag, value).map(|_| 1))
}

/// Reverts all changes.
#[no_mangle]
pub extern "C" fn elephc_zip_unchange_all(handle: i64) -> i64 {
    mutation(handle, |archive| archive.unchange_all().map(|_| 1))
}

/// Reverts one entry.
#[no_mangle]
pub extern "C" fn elephc_zip_unchange_index(handle: i64, index: i64) -> i64 {
    mutation(handle, |archive| archive.unchange_index(index as usize).map(|_| 1))
}

/// Reverts one entry by name.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_unchange_name(handle: i64, name: *const c_char, name_len: i64) -> i64 {
    let wanted = bytes(name, name_len);
    let index = with_archive(handle, None, |archive| archive.locate(&wanted, 0));
    match index { Some(index) => elephc_zip_unchange_index(handle, index as i64), None => -1 }
}

/// Reverts the archive comment and flags.
#[no_mangle]
pub extern "C" fn elephc_zip_unchange_archive(handle: i64) -> i64 {
    mutation(handle, |archive| archive.unchange_archive().map(|_| 1))
}

/// Extracts selected entries into a destination directory.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_extract(
    handle: i64, destination: *const c_char, destination_len: i64,
    selection: *const c_char, selection_len: i64,
) -> i64 {
    let destination = String::from_utf8_lossy(&bytes(destination, destination_len)).into_owned();
    let selection = crate::extract::parse_selection(&bytes(selection, selection_len));
    mutation(handle, |archive| {
        crate::extract::extract_to(archive, &destination, selection.as_deref()).map(|_| 1)
    })
}

/// Matches a PCRE-style `/pattern/flags` against a subject using the pure-Rust engine.
///
/// Returns 1 on a match, 0 on no match, and -1 for an invalid expression. The prelude
/// owns PHP's warning strings; this entry point only answers the match question.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_pattern_match(
    pattern: *const c_char, pattern_len: i64, subject: *const c_char, subject_len: i64,
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
    i64::from(crate::compress::supported_compression(method, encode != 0))
}

/// Reports whether an encryption method is available.
#[no_mangle]
pub extern "C" fn elephc_zip_encryption_supported(method: i64, encode: i64) -> i64 {
    i64::from(crate::compress::supported_encryption(method, encode != 0))
}

/// Returns the libzip compatibility version string.
#[no_mangle]
pub extern "C" fn elephc_zip_version() -> *const c_char {
    result_text(crate::engine_version())
}
