//! Purpose:
//! C ABI and in-memory handle table for elephc's `ZipArchive` bridge.
//!
//! Called from:
//! - The injected `ZipArchive` PHP prelude through `extern "elephc_zip"`.
//!
//! Key details:
//! - The raw ZIP reader lives in `elephc-phar`; this crate owns opaque archive
//!   handles, mutations, and NUL-safe output buffers for PHP strings.
//! - Output buffers are thread-local, never process-global, so concurrent
//!   programs cannot free a pointer another thread is still reading.
//! - Every entry point catches panics and reports a libzip-style error code or
//!   null pointer instead of unwinding across the C ABI.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use model::{Archive, ER_INVAL, ER_NOENT};

pub mod model;
pub mod write;

static HANDLES: OnceLock<Mutex<HashMap<i64, Archive>>> = OnceLock::new();
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

thread_local! {
    /// Last string returned to PHP. It is replaced on the next call and the
    /// prelude copies it immediately through `ptr_read_string`.
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Returns the process-wide archive handle table.
fn handles() -> &'static Mutex<HashMap<i64, Archive>> {
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Stores `bytes` in the thread-local output buffer, appends a NUL, and returns
/// the buffer pointer. The pointer stays valid until the next publish on this thread.
fn publish(bytes: &[u8]) -> *const u8 {
    OUTPUT.with(|slot| {
        let mut buffer = slot.borrow_mut();
        buffer.clear();
        buffer.extend_from_slice(bytes);
        buffer.push(0);
        buffer.as_ptr()
    })
}

/// Borrows one archive by handle for the duration of `f`.
fn with_archive<T>(handle: i64, f: impl FnOnce(&Archive) -> T) -> Option<T> {
    let guard = handles()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get(&handle).map(f)
}

/// Mutably borrows one archive by handle for the duration of `f`.
fn with_archive_mut<T>(handle: i64, f: impl FnOnce(&mut Archive) -> T) -> Option<T> {
    let mut guard = handles()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get_mut(&handle).map(f)
}

/// Converts a raw C pointer/length pair into bytes.
///
/// # Safety
/// `data` must reference `len` readable bytes when `len` is nonzero.
unsafe fn bytes<'a>(data: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        Some(&[])
    } else if data.is_null() {
        None
    } else {
        Some(std::slice::from_raw_parts(data, len))
    }
}

/// Runs `f`, converting panics and malformed inputs into `fallback`.
fn guarded<T: Copy>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// C ABI: opens an in-memory ZIP archive and returns a positive handle or `-1`.
///
/// An empty string creates a new empty archive, matching `ZipArchive::openString()`.
///
/// # Safety
/// `data` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_open_string(data: *const u8, len: usize) -> i64 {
    guarded(-1, || {
        let Some(input) = bytes(data, len) else {
            return -1;
        };
        let Some(archive) = Archive::open(input) else {
            return -1;
        };
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        handles()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, archive);
        id
    })
}

/// C ABI: drops an archive handle without serializing it.
#[no_mangle]
pub extern "C" fn elephc_zip_close(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        handles()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&handle);
    }));
}

/// C ABI: serializes and removes an archive handle, publishing the archive bytes.
///
/// Returns null when the handle is unknown or the archive cannot be serialized.
#[no_mangle]
pub extern "C" fn elephc_zip_close_string(handle: i64) -> *const u8 {
    guarded(ptr::null(), || {
        let archive = handles()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&handle);
        let Some(archive) = archive else {
            return ptr::null();
        };
        match archive.close_string() {
            Some(bytes) => publish(&bytes),
            None => ptr::null(),
        }
    })
}

/// C ABI: reports whether an archive has pending mutations (1), is unchanged
/// (0), or the handle is unknown (-1).
#[no_mangle]
pub extern "C" fn elephc_zip_is_dirty(handle: i64) -> i64 {
    guarded(-1, || {
        with_archive(handle, |archive| if archive.dirty { 1 } else { 0 }).unwrap_or(-1)
    })
}

/// C ABI: returns the number of entries, or `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_zip_num_files(handle: i64) -> i64 {
    guarded(-1, || {
        with_archive(handle, |archive| archive.num_files() as i64).unwrap_or(-1)
    })
}

/// C ABI: returns one entry's raw name, or null on failure.
#[no_mangle]
pub extern "C" fn elephc_zip_get_name(handle: i64, index: i64) -> *const u8 {
    guarded(ptr::null(), || {
        let index = usize::try_from(index).ok();
        let Some(index) = index else {
            return ptr::null();
        };
        with_archive(handle, |archive| {
            archive
                .entries
                .get(index)
                .map(|entry| publish(&entry.name))
                .unwrap_or(ptr::null())
        })
        .unwrap_or(ptr::null())
    })
}

/// C ABI: returns the first index matching the raw name, or `-1`.
///
/// # Safety
/// `name` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_locate_name(
    handle: i64,
    name: *const u8,
    len: usize,
    flags: i64,
) -> i64 {
    guarded(-1, || {
        let Some(name) = bytes(name, len) else {
            return -1;
        };
        with_archive(handle, |archive| {
            archive
                .locate(name, flags)
                .map(|index| index as i64)
                .unwrap_or(-1)
        })
        .unwrap_or(-1)
    })
}

/// C ABI: returns one entry's uncompressed payload, or null on failure.
#[no_mangle]
pub extern "C" fn elephc_zip_get_index(handle: i64, index: i64) -> *const u8 {
    guarded(ptr::null(), || {
        let index = usize::try_from(index).ok();
        let Some(index) = index else {
            return ptr::null();
        };
        with_archive(handle, |archive| {
            archive
                .payload(index)
                .map(publish)
                .unwrap_or(ptr::null())
        })
        .unwrap_or(ptr::null())
    })
}

/// C ABI: returns one entry's metadata field.
///
/// Fields: 0 size, 1 compressed size, 2 Unix mtime, 3 CRC-32, 4 method.
#[no_mangle]
pub extern "C" fn elephc_zip_stat(handle: i64, index: i64, field: i64) -> i64 {
    guarded(-1, || {
        let Ok(index) = usize::try_from(index) else {
            return -1;
        };
        with_archive(handle, |archive| {
            let Some(entry) = archive.entries.get(index) else {
                return -1;
            };
            match field {
                0 => entry
                    .data
                    .as_ref()
                    .map(|data| data.len())
                    .unwrap_or(entry.uncompressed_size) as i64,
                1 => entry.compressed.as_ref().map(|data| data.len()).unwrap_or(0) as i64,
                2 => archive.entry_mtime(index).unwrap_or(0),
                3 => i64::from(entry.crc32),
                4 => i64::from(entry.method),
                _ => -1,
            }
        })
        .unwrap_or(-1)
    })
}

/// C ABI: returns one entry's comment, or null on failure.
#[no_mangle]
pub extern "C" fn elephc_zip_get_comment_index(handle: i64, index: i64) -> *const u8 {
    guarded(ptr::null(), || {
        let Ok(index) = usize::try_from(index) else {
            return ptr::null();
        };
        with_archive(handle, |archive| {
            archive
                .entry_comment(index)
                .map(publish)
                .unwrap_or(ptr::null())
        })
        .unwrap_or(ptr::null())
    })
}

/// C ABI: returns the archive comment, or null for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_zip_get_archive_comment(handle: i64) -> *const u8 {
    guarded(ptr::null(), || {
        with_archive(handle, |archive| publish(&archive.comment)).unwrap_or(ptr::null())
    })
}

/// C ABI: adds or overwrites a file entry and returns its index or an error code.
///
/// # Safety
/// `name` and `data` must reference `name_len` / `data_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_from_string(
    handle: i64,
    name: *const u8,
    name_len: usize,
    data: *const u8,
    data_len: usize,
    flags: i64,
    mtime: i64,
) -> i64 {
    guarded(ER_INVAL, || {
        let (Some(name), Some(data)) = (bytes(name, name_len), bytes(data, data_len)) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| {
            match archive.add_file(name.to_vec(), data.to_vec(), flags, mtime) {
                Ok(_) => model::ER_OK,
                Err(code) => code,
            }
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: adds a directory entry and returns its index or an error code.
///
/// # Safety
/// `name` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_add_empty_dir(
    handle: i64,
    name: *const u8,
    len: usize,
    flags: i64,
    mtime: i64,
) -> i64 {
    guarded(ER_INVAL, || {
        let Some(name) = bytes(name, len) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| {
            match archive.add_dir(name.to_vec(), flags, mtime) {
                Ok(_) => model::ER_OK,
                Err(code) => code,
            }
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: deletes an entry by index and returns 0 or an error code.
#[no_mangle]
pub extern "C" fn elephc_zip_delete_index(handle: i64, index: i64) -> i64 {
    guarded(ER_INVAL, || {
        let Ok(index) = usize::try_from(index) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| archive.delete_index(index)).unwrap_or(ER_INVAL)
    })
}

/// C ABI: deletes an entry by name and returns 0 or an error code.
///
/// # Safety
/// `name` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_delete_name(
    handle: i64,
    name: *const u8,
    len: usize,
) -> i64 {
    guarded(ER_INVAL, || {
        let Some(name) = bytes(name, len) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| match archive.locate(name, 0) {
            Some(index) => archive.delete_index(index),
            None => ER_NOENT,
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: renames an entry by index and returns 0 or an error code.
///
/// # Safety
/// `name` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_rename_index(
    handle: i64,
    index: i64,
    name: *const u8,
    len: usize,
) -> i64 {
    guarded(ER_INVAL, || {
        let (Ok(index), Some(name)) = (usize::try_from(index), bytes(name, len)) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| archive.rename_index(index, name.to_vec()))
            .unwrap_or(ER_INVAL)
    })
}

/// C ABI: renames an entry by name and returns 0 or an error code.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_rename_name(
    handle: i64,
    old: *const u8,
    old_len: usize,
    new: *const u8,
    new_len: usize,
) -> i64 {
    guarded(ER_INVAL, || {
        let (Some(old), Some(new)) = (bytes(old, old_len), bytes(new, new_len)) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| match archive.locate(old, 0) {
            Some(index) => archive.rename_index(index, new.to_vec()),
            None => ER_NOENT,
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: sets the archive comment and returns 0 or an error code.
///
/// # Safety
/// `comment` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_archive_comment(
    handle: i64,
    comment: *const u8,
    len: usize,
) -> i64 {
    guarded(ER_INVAL, || {
        let Some(comment) = bytes(comment, len) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| {
            archive.comment = comment.to_vec();
            archive.dirty = true;
            model::ER_OK
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: sets an entry comment by index and returns 0 or an error code.
///
/// # Safety
/// `comment` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_comment_index(
    handle: i64,
    index: i64,
    comment: *const u8,
    len: usize,
) -> i64 {
    guarded(ER_INVAL, || {
        let (Ok(index), Some(comment)) = (usize::try_from(index), bytes(comment, len)) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| {
            archive.set_entry_comment(index, comment.to_vec())
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: sets an entry comment by name and returns 0 or an error code.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_comment_name(
    handle: i64,
    name: *const u8,
    name_len: usize,
    comment: *const u8,
    comment_len: usize,
) -> i64 {
    guarded(ER_INVAL, || {
        let (Some(name), Some(comment)) = (bytes(name, name_len), bytes(comment, comment_len))
        else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| match archive.locate(name, 0) {
            Some(index) => archive.set_entry_comment(index, comment.to_vec()),
            None => ER_NOENT,
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: sets an entry compression method by index.
#[no_mangle]
pub extern "C" fn elephc_zip_set_compression_index(
    handle: i64,
    index: i64,
    method: i64,
) -> i64 {
    guarded(ER_INVAL, || {
        let (Ok(index), Ok(method)) = (usize::try_from(index), u16::try_from(method)) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| archive.set_compression(index, method))
            .unwrap_or(ER_INVAL)
    })
}

/// C ABI: sets an entry compression method by name.
///
/// # Safety
/// `name` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_compression_name(
    handle: i64,
    name: *const u8,
    len: usize,
    method: i64,
) -> i64 {
    guarded(ER_INVAL, || {
        let (Some(name), Ok(method)) = (bytes(name, len), u16::try_from(method)) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| match archive.locate(name, 0) {
            Some(index) => archive.set_compression(index, method),
            None => ER_NOENT,
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: sets an entry modification time by index.
#[no_mangle]
pub extern "C" fn elephc_zip_set_mtime_index(
    handle: i64,
    index: i64,
    timestamp: i64,
) -> i64 {
    guarded(ER_INVAL, || {
        let Ok(index) = usize::try_from(index) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| archive.set_mtime(index, timestamp))
            .unwrap_or(ER_INVAL)
    })
}

/// C ABI: sets an entry modification time by name.
///
/// # Safety
/// `name` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_set_mtime_name(
    handle: i64,
    name: *const u8,
    len: usize,
    timestamp: i64,
) -> i64 {
    guarded(ER_INVAL, || {
        let Some(name) = bytes(name, len) else {
            return ER_INVAL;
        };
        with_archive_mut(handle, |archive| match archive.locate(name, 0) {
            Some(index) => archive.set_mtime(index, timestamp),
            None => ER_NOENT,
        })
        .unwrap_or(ER_INVAL)
    })
}

/// C ABI: returns the byte length of the last published output, excluding NUL.
#[no_mangle]
pub extern "C" fn elephc_zip_last_len() -> usize {
    OUTPUT.with(|slot| slot.borrow().len().saturating_sub(1))
}

/// C ABI: reports the bridge version.
#[no_mangle]
pub extern "C" fn elephc_zip_version() -> i32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{METHOD_DEFLATE, METHOD_STORE};

    /// A committed two-entry ZIP fixture.
    const SAMPLE: &[u8] = include_bytes!("../tests/fixtures/sample.zip");

    /// Opening the fixture exposes both entries and extracts deflated/stored data.
    #[test]
    fn open_extract_and_locate_round_trip() {
        let archive = Archive::open(SAMPLE).expect("sample opens");
        assert_eq!(archive.num_files(), 2);
        assert_eq!(archive.entries[0].name, b"hello.txt");
        assert_eq!(archive.entries[0].method, METHOD_DEFLATE);
        assert_eq!(archive.payload(0), Some(&b"Hello, ZIP!\n"[..]));
        assert_eq!(archive.locate(b"stored.txt", 0), Some(1));
        assert_eq!(archive.entries[1].method, METHOD_STORE);
        assert_eq!(archive.payload(1), Some(&b"stored\n"[..]));
    }

    /// Non-ZIP bytes fail closed instead of producing an archive.
    #[test]
    fn rejects_non_zip_bytes() {
        assert!(Archive::open(b"not a zip").is_none());
    }

    /// Mutations round-trip through the writer with correct payloads and metadata.
    #[test]
    fn mutations_reserialize_round_trip() {
        let mut archive = Archive::open(SAMPLE).expect("sample opens");
        let index = archive
            .add_file(b"added.txt".to_vec(), b"added\n".to_vec(), model::FL_OVERWRITE, 1_700_000_000)
            .expect("add succeeds");
        assert_eq!(index, 2);
        assert_eq!(archive.delete_index(1), model::ER_OK);
        assert_eq!(archive.rename_index(0, b"renamed.txt".to_vec()), model::ER_OK);
        assert_eq!(archive.set_entry_comment(0, b"comment".to_vec()), model::ER_OK);
        let bytes = archive.close_string().expect("serializes");
        let reopened = Archive::open(&bytes).expect("reserialized archive opens");
        assert_eq!(reopened.num_files(), 2);
        assert_eq!(reopened.entries[0].name, b"renamed.txt");
        assert_eq!(reopened.payload(0), Some(&b"Hello, ZIP!\n"[..]));
        assert_eq!(reopened.entry_comment(0), Some(&b"comment"[..]));
        assert_eq!(reopened.entries[1].name, b"added.txt");
        assert_eq!(reopened.payload(1), Some(&b"added\n"[..]));
    }

    /// An untouched archive is returned byte-for-byte.
    #[test]
    fn unchanged_archive_returns_original_bytes() {
        let archive = Archive::open(SAMPLE).expect("sample opens");
        assert_eq!(archive.close_string().as_deref(), Some(SAMPLE));
    }

    /// The empty constructor serializes a valid empty ZIP.
    #[test]
    fn empty_archive_serializes() {
        let archive = Archive::empty();
        let bytes = archive.close_string().expect("serializes");
        let reopened = Archive::open(&bytes).expect("empty archive opens");
        assert_eq!(reopened.num_files(), 0);
    }
}
