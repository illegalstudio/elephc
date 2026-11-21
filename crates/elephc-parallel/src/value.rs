//! Purpose:
//! Defines the versioned, pointer-free value wire format used by Parallel jobs.
//!
//! Called from:
//! - Parent-side capture/argument encoders and worker-side result encoders.
//! - Worker and Future decoders before allocating values in their local `_rt_ctx`.
//!
//! Key details:
//! - The format supports only the statically transferable v1 value set.
//! - Writers enforce exactly one structurally complete root value.
//! - Readers validate the whole envelope before exposing borrowed byte slices.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use crate::{
    copy_transfer_payload, transfer_blob_is_valid, ParallelBytes, ParallelTransferBlob,
    PARALLEL_TRANSFER_ALLOCATION_FAILED, PARALLEL_TRANSFER_INVALID_ARGUMENT,
    PARALLEL_TRANSFER_INVALID_ENVELOPE, PARALLEL_TRANSFER_OK, PARALLEL_TRANSFER_PANIC,
    TRANSFER_HEADER_LEN,
};

const VALUE_MAGIC: [u8; 8] = *b"EPV\x01\0\0\0\0";
const MAX_VALUE_DEPTH: usize = 256;

pub const PARALLEL_VALUE_NULL: i32 = 1;
pub const PARALLEL_VALUE_FALSE: i32 = 2;
pub const PARALLEL_VALUE_TRUE: i32 = 3;
pub const PARALLEL_VALUE_INT: i32 = 4;
pub const PARALLEL_VALUE_FLOAT: i32 = 5;
pub const PARALLEL_VALUE_STRING: i32 = 6;
pub const PARALLEL_VALUE_INDEXED_ARRAY: i32 = 7;
pub const PARALLEL_VALUE_ASSOC_ARRAY: i32 = 8;
pub const PARALLEL_VALUE_CANCELLATION: i32 = 9;

#[repr(C)]
pub struct ParallelValueWriter {
    bytes: Vec<u8>,
    remaining: Vec<usize>,
    complete: bool,
}

impl ParallelValueWriter {
    /// Initializes the EPV version header and one outstanding root slot, reporting reserve failure.
    fn new() -> Result<Self, i32> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(VALUE_MAGIC.len())
            .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
        bytes.extend_from_slice(&VALUE_MAGIC);
        Ok(Self {
            bytes,
            remaining: vec![1],
            complete: false,
        })
    }

    /// Consumes one structural slot and appends its scalar tag/payload without introducing pointers.
    fn write_scalar(&mut self, tag: i32, payload: &[u8]) -> Result<(), i32> {
        self.consume_value(0)?;
        self.bytes
            .try_reserve_exact(1 + payload.len())
            .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
        self.bytes.push(tag as u8);
        self.bytes.extend_from_slice(payload);
        Ok(())
    }

    /// Appends a container count and schedules its required children after consuming its parent slot.
    fn write_container(&mut self, tag: i32, count: usize, slots: usize) -> Result<(), i32> {
        let count = u64::try_from(count).map_err(|_| PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
        self.consume_value(slots)?;
        self.bytes
            .try_reserve_exact(9)
            .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
        self.bytes.push(tag as u8);
        self.bytes.extend_from_slice(&count.to_le_bytes());
        Ok(())
    }

    /// Advances structural completion, rejecting extra roots, exhausted parents and excessive depth.
    fn consume_value(&mut self, children: usize) -> Result<(), i32> {
        if self.complete {
            return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
        }
        while self.remaining.last().is_some_and(|remaining| *remaining == 0) {
            self.remaining.pop();
        }
        let Some(remaining) = self.remaining.last_mut() else {
            return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
        };
        *remaining = remaining
            .checked_sub(1)
            .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
        if children != 0 {
            if self.remaining.len() >= MAX_VALUE_DEPTH {
                return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
            }
            self.remaining.push(children);
        }
        while self.remaining.last().is_some_and(|remaining| *remaining == 0) {
            self.remaining.pop();
        }
        self.complete = self.remaining.is_empty();
        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParallelValueView {
    pub tag: i32,
    pub int_value: i64,
    pub float_bits: u64,
    pub bytes: ParallelBytes,
    pub count: usize,
    pub cancellation_id: u64,
}

impl ParallelValueView {
    pub(crate) const EMPTY: Self = Self {
        tag: 0,
        int_value: 0,
        float_bits: 0,
        bytes: ParallelBytes {
            ptr: ptr::null(),
            len: 0,
        },
        count: 0,
        cancellation_id: 0,
    };
}

/// Borrows an outer-envelope payload; callers must keep its allocation alive for every returned view.
fn payload(blob: ParallelTransferBlob) -> Option<&'static [u8]> {
    if !transfer_blob_is_valid(blob) {
        return None;
    }
    Some(unsafe {
        std::slice::from_raw_parts(blob.ptr.add(TRANSFER_HEADER_LEN), blob.len - TRANSFER_HEADER_LEN)
    })
}

/// Reads a bounded little-endian word and advances the cursor only when all eight bytes exist.
fn read_u64(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let end = cursor.checked_add(8)?;
    let raw: [u8; 8] = bytes.get(*cursor..end)?.try_into().ok()?;
    *cursor = end;
    Some(u64::from_le_bytes(raw))
}

/// Checks one complete value/subtree with bounded depth and advances past all of its children.
fn skip_value(bytes: &[u8], cursor: &mut usize, depth: usize) -> Option<()> {
    if depth >= MAX_VALUE_DEPTH {
        return None;
    }
    let tag = *bytes.get(*cursor)?;
    *cursor += 1;
    match i32::from(tag) {
        PARALLEL_VALUE_NULL | PARALLEL_VALUE_FALSE | PARALLEL_VALUE_TRUE => Some(()),
        PARALLEL_VALUE_INT | PARALLEL_VALUE_FLOAT | PARALLEL_VALUE_CANCELLATION => {
            let value = read_u64(bytes, cursor)?;
            if i32::from(tag) == PARALLEL_VALUE_CANCELLATION && value == 0 {
                return None;
            }
            Some(())
        }
        PARALLEL_VALUE_STRING => {
            let len = usize::try_from(read_u64(bytes, cursor)?).ok()?;
            let end = cursor.checked_add(len)?;
            bytes.get(*cursor..end)?;
            *cursor = end;
            Some(())
        }
        PARALLEL_VALUE_INDEXED_ARRAY | PARALLEL_VALUE_ASSOC_ARRAY => {
            let count = usize::try_from(read_u64(bytes, cursor)?).ok()?;
            let slots = if i32::from(tag) == PARALLEL_VALUE_ASSOC_ARRAY {
                count.checked_mul(2)?
            } else {
                count
            };
            for _ in 0..slots {
                skip_value(bytes, cursor, depth + 1)?;
            }
            Some(())
        }
        _ => None,
    }
}

/// Requires the EPV header and exactly one structurally valid root with no trailing bytes.
pub(crate) fn value_envelope_is_valid(blob: ParallelTransferBlob) -> bool {
    let Some(bytes) = payload(blob) else {
        return false;
    };
    if bytes.len() < VALUE_MAGIC.len() || bytes[..VALUE_MAGIC.len()] != VALUE_MAGIC {
        return false;
    }
    let mut cursor = VALUE_MAGIC.len();
    skip_value(bytes, &mut cursor, 0).is_some() && cursor == bytes.len()
}

/// Validates the writer handle and contains Rust panics while mapping one mutation to ABI status.
fn writer_status(
    writer: *mut ParallelValueWriter,
    action: impl FnOnce(&mut ParallelValueWriter) -> Result<(), i32>,
) -> i32 {
    if writer.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    match catch_unwind(AssertUnwindSafe(|| action(unsafe { &mut *writer }))) {
        Ok(Ok(())) => PARALLEL_TRANSFER_OK,
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Allocates a single-root writer, returning null on initialization failure or panic.
#[no_mangle]
pub extern "C" fn elephc_parallel_value_writer_new() -> *mut ParallelValueWriter {
    match catch_unwind(ParallelValueWriter::new) {
        Ok(Ok(writer)) => Box::into_raw(Box::new(writer)),
        _ => ptr::null_mut(),
    }
}

/// Destroys a writer not consumed by finish; a null handle is ignored.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
pub unsafe extern "C" fn elephc_parallel_value_writer_free(writer: *mut ParallelValueWriter) {
    if !writer.is_null() {
        drop(unsafe { Box::from_raw(writer) });
    }
}

/// Appends one null value while preserving the writer's single-root structural contract.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
///
/// Safe Rust callers cannot invoke the raw-pointer API without acknowledging its contract:
///
/// ```compile_fail
/// let writer = elephc_parallel::elephc_parallel_value_writer_new();
/// let _ = elephc_parallel::elephc_parallel_value_write_null(writer);
/// ```
pub unsafe extern "C" fn elephc_parallel_value_write_null(writer: *mut ParallelValueWriter) -> i32 {
    writer_status(writer, |writer| writer.write_scalar(PARALLEL_VALUE_NULL, &[]))
}

/// Encodes zero as false and any nonzero C boolean as true in the next value slot.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
pub unsafe extern "C" fn elephc_parallel_value_write_bool(
    writer: *mut ParallelValueWriter,
    value: i32,
) -> i32 {
    let tag = if value == 0 {
        PARALLEL_VALUE_FALSE
    } else {
        PARALLEL_VALUE_TRUE
    };
    writer_status(writer, |writer| writer.write_scalar(tag, &[]))
}

/// Appends a signed integer as its pointer-free little-endian payload.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
pub unsafe extern "C" fn elephc_parallel_value_write_int(
    writer: *mut ParallelValueWriter,
    value: i64,
) -> i32 {
    writer_status(writer, |writer| {
        writer.write_scalar(PARALLEL_VALUE_INT, &value.to_le_bytes())
    })
}

/// Appends exact binary64 bits, preserving signed zero and special floating-point encodings.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
pub unsafe extern "C" fn elephc_parallel_value_write_float(
    writer: *mut ParallelValueWriter,
    bits: u64,
) -> i32 {
    writer_status(writer, |writer| {
        writer.write_scalar(PARALLEL_VALUE_FLOAT, &bits.to_le_bytes())
    })
}

/// Copies a counted byte string into the next slot, rejecting invalid pointer/length combinations.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`. If `len` is nonzero, `value` must be
/// readable for `len` bytes.
pub unsafe extern "C" fn elephc_parallel_value_write_string(
    writer: *mut ParallelValueWriter,
    value: *const u8,
    len: usize,
) -> i32 {
    if value.is_null() && len != 0 {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    let Ok(len_word) = u64::try_from(len) else {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    };
    writer_status(writer, |writer| {
        writer.consume_value(0)?;
        writer
            .bytes
            .try_reserve_exact(9usize.saturating_add(len))
            .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
        writer.bytes.push(PARALLEL_VALUE_STRING as u8);
        writer.bytes.extend_from_slice(&len_word.to_le_bytes());
        if len != 0 {
            writer
                .bytes
                .extend_from_slice(unsafe { std::slice::from_raw_parts(value, len) });
        }
        Ok(())
    })
}

/// Begins an indexed container whose following values must fill exactly count child slots.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
pub unsafe extern "C" fn elephc_parallel_value_begin_indexed(
    writer: *mut ParallelValueWriter,
    count: usize,
) -> i32 {
    writer_status(writer, |writer| {
        writer.write_container(PARALLEL_VALUE_INDEXED_ARRAY, count, count)
    })
}

/// Begins an associative container with checked space for count key/value pairs.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
pub unsafe extern "C" fn elephc_parallel_value_begin_assoc(
    writer: *mut ParallelValueWriter,
    count: usize,
) -> i32 {
    let Some(slots) = count.checked_mul(2) else {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    };
    writer_status(writer, |writer| {
        writer.write_container(PARALLEL_VALUE_ASSOC_ARRAY, count, slots)
    })
}

/// Appends a nonzero cancellation identity without transferring a native token pointer.
#[no_mangle]
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed by
/// `elephc_parallel_value_writer_finish`.
pub unsafe extern "C" fn elephc_parallel_value_write_cancellation(
    writer: *mut ParallelValueWriter,
    id: u64,
) -> i32 {
    if id == 0 {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    writer_status(writer, |writer| {
        writer.write_scalar(PARALLEL_VALUE_CANCELLATION, &id.to_le_bytes())
    })
}

/// Consumes a writer into an owned transfer blob, requiring a complete root and clearing output first.
/// Once a nonnull output is accepted, the writer is consumed even on incomplete/envelope-copy failure.
#[no_mangle]
/// # Safety
/// `output` must be writable for one `ParallelTransferBlob`. `writer` must be a
/// live, uniquely owned writer returned by `elephc_parallel_value_writer_new`
/// and not yet freed or consumed.
pub unsafe extern "C" fn elephc_parallel_value_writer_finish(
    writer: *mut ParallelValueWriter,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelTransferBlob::EMPTY) };
    if writer.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    let writer = unsafe { Box::from_raw(writer) };
    if !writer.complete {
        return PARALLEL_TRANSFER_INVALID_ENVELOPE;
    }
    match copy_transfer_payload(writer.bytes.as_ptr(), writer.bytes.len()) {
        Ok(blob) => {
            unsafe { output.write(blob) };
            PARALLEL_TRANSFER_OK
        }
        Err(status) => status,
    }
}

/// Checks the entire value envelope and returns structural validity without allocating decoded values.
#[no_mangle]
/// # Safety
/// If `blob.len` is nonzero, `blob.ptr` must be readable for `blob.len` bytes
/// for the duration of this call.
///
/// ```compile_fail
/// let blob = elephc_parallel::ParallelTransferBlob {
///     ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(),
///     len: 8,
/// };
/// let _ = elephc_parallel::elephc_parallel_value_validate(blob);
/// ```
pub unsafe extern "C" fn elephc_parallel_value_validate(blob: ParallelTransferBlob) -> i32 {
    if value_envelope_is_valid(blob) {
        PARALLEL_TRANSFER_OK
    } else {
        PARALLEL_TRANSFER_INVALID_ENVELOPE
    }
}

/// Validates an envelope before positioning the reader just after its version header.
#[no_mangle]
/// # Safety
/// If `blob.len` is nonzero, `blob.ptr` must be readable for `blob.len` bytes.
/// `cursor` must be writable for one `usize`.
pub unsafe extern "C" fn elephc_parallel_value_reader_begin(
    blob: ParallelTransferBlob,
    cursor: *mut usize,
) -> i32 {
    if cursor.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { cursor.write(0) };
    if !value_envelope_is_valid(blob) {
        return PARALLEL_TRANSFER_INVALID_ENVELOPE;
    }
    unsafe { cursor.write(VALUE_MAGIC.len()) };
    PARALLEL_TRANSFER_OK
}

/// Reads one value header/scalar; container children remain at the cursor for subsequent reads.
/// String byte views borrow the blob and become invalid when its owning allocation is freed.
#[no_mangle]
/// # Safety
/// If `blob.len` is nonzero, `blob.ptr` must be readable for `blob.len` bytes.
/// `cursor` must be readable and writable for one `usize`, and `output` must be
/// writable for one `ParallelValueView`.
pub unsafe extern "C" fn elephc_parallel_value_read(
    blob: ParallelTransferBlob,
    cursor: *mut usize,
    output: *mut ParallelValueView,
) -> i32 {
    if cursor.is_null() || output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelValueView::EMPTY) };
    let Some(bytes) = payload(blob) else {
        return PARALLEL_TRANSFER_INVALID_ENVELOPE;
    };
    let mut at = unsafe { cursor.read() };
    if at < VALUE_MAGIC.len() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    let Some(tag) = bytes.get(at).copied().map(i32::from) else {
        return PARALLEL_TRANSFER_INVALID_ENVELOPE;
    };
    at += 1;
    let mut view = ParallelValueView {
        tag,
        ..ParallelValueView::EMPTY
    };
    match tag {
        PARALLEL_VALUE_NULL | PARALLEL_VALUE_FALSE | PARALLEL_VALUE_TRUE => {}
        PARALLEL_VALUE_INT => {
            let Some(value) = read_u64(bytes, &mut at) else {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            };
            view.int_value = i64::from_le_bytes(value.to_le_bytes());
        }
        PARALLEL_VALUE_FLOAT => {
            let Some(bits) = read_u64(bytes, &mut at) else {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            };
            view.float_bits = bits;
        }
        PARALLEL_VALUE_STRING => {
            let Some(len) = read_u64(bytes, &mut at).and_then(|len| usize::try_from(len).ok()) else {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            };
            let Some(end) = at.checked_add(len) else {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            };
            let Some(value) = bytes.get(at..end) else {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            };
            view.bytes = ParallelBytes {
                ptr: value.as_ptr(),
                len: value.len(),
            };
            at = end;
        }
        PARALLEL_VALUE_INDEXED_ARRAY | PARALLEL_VALUE_ASSOC_ARRAY => {
            let Some(count) = read_u64(bytes, &mut at).and_then(|len| usize::try_from(len).ok()) else {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            };
            view.count = count;
        }
        PARALLEL_VALUE_CANCELLATION => {
            let Some(id) = read_u64(bytes, &mut at) else {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            };
            if id == 0 {
                return PARALLEL_TRANSFER_INVALID_ENVELOPE;
            }
            view.cancellation_id = id;
        }
        _ => return PARALLEL_TRANSFER_INVALID_ENVELOPE,
    }
    unsafe {
        cursor.write(at);
        output.write(view);
    }
    PARALLEL_TRANSFER_OK
}

#[cfg(test)]
mod tests;
