//! Purpose:
//! FFI result publication, URL splitting, and buffered write-stream bookkeeping.
//!
//! Called from:
//! - The PHAR C ABI wrapper modules.
//!
//! Key details:
//! - Stream slots are bounded and released before final archive mutation.

use super::*;

/// Builds a byte slice from a C pointer and byte length.
///
/// A zero length never dereferences the pointer, so null plus zero is accepted.
pub(super) unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// Stores extracted bytes in the calling thread's result buffer and returns its
/// pointer, valid until that thread's next publish. The buffer is per-thread
/// because refilling it reallocates: one process-global buffer would free the
/// bytes another thread was just handed a pointer into.
pub(super) fn publish_result(bytes: Vec<u8>, out_len: *mut usize) -> *const u8 {
    EXTRACT_BUFFER.with(|slot| {
        let mut buffer = slot.borrow_mut();
        buffer.clear();
        buffer.extend_from_slice(&bytes);
        write_len(out_len, buffer.len());
        if buffer.is_empty() {
            b"".as_ptr()
        } else {
            buffer.as_ptr()
        }
    })
}

/// Returns the process-global table for buffered PHAR write streams.
pub(super) fn write_streams() -> &'static Mutex<Vec<Option<WriteStream>>> {
    WRITE_STREAMS.get_or_init(|| {
        let mut streams = Vec::with_capacity(PHAR_WRITE_STREAM_LIMIT);
        streams.resize_with(PHAR_WRITE_STREAM_LIMIT, || None);
        Mutex::new(streams)
    })
}

/// Allocates a write-stream slot and returns its synthetic descriptor.
pub(super) fn allocate_write_stream(stream: WriteStream) -> Option<usize> {
    let mut streams = write_streams().lock().ok()?;
    for (slot, current) in streams.iter_mut().enumerate() {
        if current.is_none() {
            *current = Some(stream);
            return Some(PHAR_WRITE_FD_BASE + slot);
        }
    }
    None
}

/// Converts a synthetic PHAR descriptor into a write-stream slot index.
pub(super) fn write_stream_slot(fd: usize) -> Option<usize> {
    let slot = fd.checked_sub(PHAR_WRITE_FD_BASE)?;
    (slot < PHAR_WRITE_STREAM_LIMIT).then_some(slot)
}

/// Appends payload bytes to an open write stream.
pub(super) fn append_write_stream(fd: usize, data: &[u8]) -> Option<usize> {
    let slot = write_stream_slot(fd)?;
    let mut streams = write_streams().lock().ok()?;
    let stream = streams.get_mut(slot)?.as_mut()?;
    stream.payload.extend_from_slice(data);
    Some(data.len())
}

/// Finalizes one open write stream and writes its target archive.
pub(super) fn finalize_write_stream(fd: usize) -> Option<()> {
    let slot = write_stream_slot(fd)?;
    let stream = {
        let mut streams = write_streams().lock().ok()?;
        streams.get_mut(slot)?.take()?
    };
    match stream.target {
        WriteStreamTarget::Entry { archive, entry } => {
            put_entry_bytes(&archive, &entry, &stream.payload)?;
        }
        WriteStreamTarget::Url(url) => {
            put_url_bytes(&url, &stream.payload)?;
        }
    }
    Some(())
}

/// Writes an output length through the optional C pointer.
pub(super) fn write_len(out_len: *mut usize, len: usize) {
    if !out_len.is_null() {
        unsafe {
            *out_len = len;
        }
    }
}

/// Splits `phar://` URL body bytes into an existing archive path and inner entry name.
pub(super) fn split_archive_entry(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    for (i, &byte) in rest.iter().enumerate() {
        if byte != b'/' || i == 0 || i + 1 >= rest.len() {
            continue;
        }
        let candidate = std::str::from_utf8(&rest[..i]).ok()?;
        if std::path::Path::new(candidate).is_file() {
            return Some((&rest[..i], &rest[i + 1..]));
        }
    }
    None
}

/// Splits `phar://` URL body bytes for writes, including missing archives.
pub(super) fn split_write_url_entry(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    for suffix in [b".phar/".as_slice(), b".tar/".as_slice(), b".zip/".as_slice()] {
        if let Some(idx) = find_subslice(rest, suffix) {
            let split = idx.checked_add(suffix.len().checked_sub(1)?)?;
            return Some((rest.get(..split)?, rest.get(split + 1..)?));
        }
    }
    let idx = rest.iter().rposition(|&byte| byte == b'/')?;
    if idx == 0 || idx + 1 >= rest.len() {
        return None;
    }
    Some((rest.get(..idx)?, rest.get(idx + 1..)?))
}
