//! Purpose:
//! Encodes and validates versioned, pointer-free Parallel worker failures.
//!
//! Called from:
//! - Worker completion paths before publishing a failed native job.
//! - Parent-side Future joins while reconstructing `TaskFailure` data.
//!
//! Key details:
//! - Every pointer in the C views borrows bytes from one owned transfer blob.
//! - Optional strings use a null pointer on input and an explicit sentinel on wire.
//! - Throwable objects and runtime-context pointers never enter the envelope.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use crate::{
    copy_transfer_payload, transfer_blob_is_valid, ParallelTransferBlob,
    PARALLEL_TRANSFER_ALLOCATION_FAILED, PARALLEL_TRANSFER_INVALID_ARGUMENT,
    PARALLEL_TRANSFER_INVALID_ENVELOPE, PARALLEL_TRANSFER_OK, PARALLEL_TRANSFER_PANIC,
    TRANSFER_HEADER_LEN,
};

const FAILURE_MAGIC: [u8; 8] = *b"EPFL\x01\0\0\0";
const FAILURE_HEADER_LEN: usize = 40;
const FRAME_HEADER_LEN: usize = 12;
const OPTIONAL_NONE: u32 = u32::MAX;

pub const PARALLEL_FAILURE_PHP_THROWABLE: i32 = 1;
pub const PARALLEL_FAILURE_PHP_FATAL: i32 = 2;
pub const PARALLEL_FAILURE_WORKER_PANIC: i32 = 3;
pub const PARALLEL_FAILURE_CONTEXT_UNAVAILABLE: i32 = 4;
pub const PARALLEL_FAILURE_ARENA_EXHAUSTED: i32 = 5;
pub const PARALLEL_FAILURE_TRANSFER_ENCODE: i32 = 6;
pub const PARALLEL_FAILURE_TRANSFER_DECODE: i32 = 7;
pub const PARALLEL_FAILURE_INFRASTRUCTURE: i32 = 8;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParallelBytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl ParallelBytes {
    const EMPTY: Self = Self {
        ptr: ptr::null(),
        len: 0,
    };
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParallelFailureFrameInput {
    pub function: ParallelBytes,
    pub file: ParallelBytes,
    pub line: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParallelFailureInput {
    pub kind: i32,
    pub remote_class: ParallelBytes,
    pub message: ParallelBytes,
    pub code: i64,
    pub file: ParallelBytes,
    pub line: u32,
    pub frames: *const ParallelFailureFrameInput,
    pub frame_count: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParallelFailureView {
    pub kind: i32,
    pub remote_class: ParallelBytes,
    pub message: ParallelBytes,
    pub code: i64,
    pub file: ParallelBytes,
    pub line: u32,
    pub frame_count: usize,
}

impl ParallelFailureView {
    const EMPTY: Self = Self {
        kind: 0,
        remote_class: ParallelBytes::EMPTY,
        message: ParallelBytes::EMPTY,
        code: 0,
        file: ParallelBytes::EMPTY,
        line: 0,
        frame_count: 0,
    };
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParallelFailureFrameView {
    pub function: ParallelBytes,
    pub file: ParallelBytes,
    pub line: u32,
}

impl ParallelFailureFrameView {
    const EMPTY: Self = Self {
        function: ParallelBytes::EMPTY,
        file: ParallelBytes::EMPTY,
        line: 0,
    };
}

#[derive(Clone, Copy)]
struct ParsedFrame<'a> {
    function: &'a [u8],
    file: Option<&'a [u8]>,
    line: u32,
}

struct ParsedFailure<'a> {
    kind: i32,
    remote_class: Option<&'a [u8]>,
    message: &'a [u8],
    code: i64,
    file: Option<&'a [u8]>,
    line: u32,
    frames: Vec<ParsedFrame<'a>>,
}

/// Accepts only the version-one failure categories understood by worker and parent decoders.
fn valid_kind(kind: i32) -> bool {
    matches!(
        kind,
        PARALLEL_FAILURE_PHP_THROWABLE
            | PARALLEL_FAILURE_PHP_FATAL
            | PARALLEL_FAILURE_WORKER_PANIC
            | PARALLEL_FAILURE_CONTEXT_UNAVAILABLE
            | PARALLEL_FAILURE_ARENA_EXHAUSTED
            | PARALLEL_FAILURE_TRANSFER_ENCODE
            | PARALLEL_FAILURE_TRANSFER_DECODE
            | PARALLEL_FAILURE_INFRASTRUCTURE
    )
}

/// Borrows required input bytes, allowing null only for empty data; storage must remain readable.
unsafe fn required_bytes(value: ParallelBytes) -> Option<&'static [u8]> {
    if value.ptr.is_null() {
        return (value.len == 0).then_some(&[]);
    }
    Some(unsafe { std::slice::from_raw_parts(value.ptr, value.len) })
}

/// Distinguishes absent null/zero input from a present byte slice without transferring ownership.
unsafe fn optional_bytes(value: ParallelBytes) -> Option<Option<&'static [u8]>> {
    if value.ptr.is_null() {
        return (value.len == 0).then_some(None);
    }
    Some(Some(unsafe {
        std::slice::from_raw_parts(value.ptr, value.len)
    }))
}

/// Checks that an input length fits the wire header before narrowing its native usize.
fn wire_u32_len(len: usize) -> Option<u32> {
    u32::try_from(len).ok()
}

/// Counts optional payload bytes while rejecting a present length that collides with the None sentinel.
fn encoded_len(value: Option<&[u8]>) -> Option<usize> {
    match value {
        None => Some(0),
        Some(bytes) => {
            let len = wire_u32_len(bytes.len())?;
            (len != OPTIONAL_NONE).then_some(bytes.len())
        }
    }
}

/// Appends a little-endian unsigned header word to the owned failure payload.
fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

/// Appends a signed failure code as its exact little-endian integer bits.
fn push_i64(output: &mut Vec<u8>, value: i64) {
    output.extend_from_slice(&value.to_le_bytes());
}

/// Writes a checked present-string length or the explicit absent-string sentinel.
fn push_optional_len(output: &mut Vec<u8>, value: Option<&[u8]>) {
    push_u32(
        output,
        value.map_or(OPTIONAL_NONE, |bytes| bytes.len() as u32),
    );
}

/// Copies validated failure fields/frames into a pointer-free owned EPFL transfer envelope.
/// The caller must keep every nonnull input slice and frame array readable during encoding.
pub(crate) unsafe fn encode_failure(
    input: &ParallelFailureInput,
) -> Result<ParallelTransferBlob, i32> {
    if !valid_kind(input.kind) || input.frame_count > u32::MAX as usize {
        return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
    }
    let remote_class = unsafe { optional_bytes(input.remote_class) }
        .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
    let message = unsafe { required_bytes(input.message) }
        .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
    let message_len = wire_u32_len(message.len()).ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
    let file = unsafe { optional_bytes(input.file) }.ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
    if input.frames.is_null() && input.frame_count != 0 {
        return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
    }
    let frames = if input.frame_count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(input.frames, input.frame_count) }
    };

    let mut total = FAILURE_HEADER_LEN
        .checked_add(encoded_len(remote_class).ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?)
        .and_then(|len| len.checked_add(message.len()))
        .and_then(|len| len.checked_add(encoded_len(file)?))
        .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
    let mut parsed_frames = Vec::new();
    parsed_frames
        .try_reserve_exact(frames.len())
        .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
    for frame in frames {
        let function = unsafe { required_bytes(frame.function) }
            .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
        let file = unsafe { optional_bytes(frame.file) }
            .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
        let function_len = wire_u32_len(function.len())
            .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
        if function_len == OPTIONAL_NONE {
            return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
        }
        total = total
            .checked_add(FRAME_HEADER_LEN)
            .and_then(|len| len.checked_add(function.len()))
            .and_then(|len| len.checked_add(encoded_len(file)?))
            .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
        parsed_frames.push((function, function_len, file, frame.line));
    }

    let mut payload = Vec::new();
    payload
        .try_reserve_exact(total)
        .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
    payload.extend_from_slice(&FAILURE_MAGIC);
    push_u32(&mut payload, input.kind as u32);
    push_i64(&mut payload, input.code);
    push_u32(&mut payload, input.line);
    push_optional_len(&mut payload, remote_class);
    push_u32(&mut payload, message_len);
    push_optional_len(&mut payload, file);
    push_u32(&mut payload, frames.len() as u32);
    if let Some(bytes) = remote_class {
        payload.extend_from_slice(bytes);
    }
    payload.extend_from_slice(message);
    if let Some(bytes) = file {
        payload.extend_from_slice(bytes);
    }
    for (function, function_len, file, line) in parsed_frames {
        push_u32(&mut payload, function_len);
        push_optional_len(&mut payload, file);
        push_u32(&mut payload, line);
        payload.extend_from_slice(function);
        if let Some(bytes) = file {
            payload.extend_from_slice(bytes);
        }
    }
    copy_transfer_payload(payload.as_ptr(), payload.len())
}

/// Reads one bounded little-endian header word, advancing only after a complete read.
fn read_u32(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let end = cursor.checked_add(4)?;
    let raw: [u8; 4] = bytes.get(*cursor..end)?.try_into().ok()?;
    *cursor = end;
    Some(u32::from_le_bytes(raw))
}

/// Reads an exact signed failure code from eight bounded payload bytes.
fn read_i64(bytes: &[u8], cursor: &mut usize) -> Option<i64> {
    let end = cursor.checked_add(8)?;
    let raw: [u8; 8] = bytes.get(*cursor..end)?.try_into().ok()?;
    *cursor = end;
    Some(i64::from_le_bytes(raw))
}

/// Borrows a counted payload slice with checked cursor addition and no allocation.
fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: u32) -> Option<&'a [u8]> {
    let len = usize::try_from(len).ok()?;
    let end = cursor.checked_add(len)?;
    let value = bytes.get(*cursor..end)?;
    *cursor = end;
    Some(value)
}

/// Reads an optional string, preserving absence separately from a present empty slice.
fn take_optional<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: u32,
) -> Option<Option<&'a [u8]>> {
    if len == OPTIONAL_NONE {
        Some(None)
    } else {
        take(bytes, cursor, len).map(Some)
    }
}

/// Validates kind, lengths, frame records and exact envelope exhaustion before exposing borrowed views.
/// All returned slices borrow the caller's still-live blob allocation.
fn parse_failure(blob: ParallelTransferBlob) -> Option<ParsedFailure<'static>> {
    if !transfer_blob_is_valid(blob) {
        return None;
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(blob.ptr.add(TRANSFER_HEADER_LEN), blob.len - TRANSFER_HEADER_LEN)
    };
    if bytes.len() < FAILURE_HEADER_LEN || bytes[..FAILURE_MAGIC.len()] != FAILURE_MAGIC {
        return None;
    }
    let mut cursor = FAILURE_MAGIC.len();
    let kind = i32::try_from(read_u32(bytes, &mut cursor)?).ok()?;
    if !valid_kind(kind) {
        return None;
    }
    let code = read_i64(bytes, &mut cursor)?;
    let line = read_u32(bytes, &mut cursor)?;
    let remote_class_len = read_u32(bytes, &mut cursor)?;
    let message_len = read_u32(bytes, &mut cursor)?;
    let file_len = read_u32(bytes, &mut cursor)?;
    let frame_count = usize::try_from(read_u32(bytes, &mut cursor)?).ok()?;
    let remote_class = take_optional(bytes, &mut cursor, remote_class_len)?;
    let message = take(bytes, &mut cursor, message_len)?;
    let file = take_optional(bytes, &mut cursor, file_len)?;
    let mut frames = Vec::new();
    frames.try_reserve_exact(frame_count).ok()?;
    for _ in 0..frame_count {
        let function_len = read_u32(bytes, &mut cursor)?;
        if function_len == OPTIONAL_NONE {
            return None;
        }
        let frame_file_len = read_u32(bytes, &mut cursor)?;
        let frame_line = read_u32(bytes, &mut cursor)?;
        let function = take(bytes, &mut cursor, function_len)?;
        let frame_file = take_optional(bytes, &mut cursor, frame_file_len)?;
        frames.push(ParsedFrame {
            function,
            file: frame_file,
            line: frame_line,
        });
    }
    (cursor == bytes.len()).then_some(ParsedFailure {
        kind,
        remote_class,
        message,
        code,
        file,
        line,
        frames,
    })
}

/// Appends PHP serialized integer syntax for one already validated scalar field.
fn push_php_int(output: &mut Vec<u8>, value: i64) {
    output.extend_from_slice(b"i:");
    output.extend_from_slice(value.to_string().as_bytes());
    output.push(b';');
}

/// Appends a byte-length-prefixed PHP serialized string without interpreting its content.
fn push_php_string(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(b"s:");
    output.extend_from_slice(value.len().to_string().as_bytes());
    output.extend_from_slice(b":\"");
    output.extend_from_slice(value);
    output.extend_from_slice(b"\";");
}

/// Serializes an optional failure field as either a counted string or PHP null.
fn push_php_optional_string(output: &mut Vec<u8>, value: Option<&[u8]>) {
    if let Some(value) = value {
        push_php_string(output, value);
    } else {
        output.extend_from_slice(b"N;");
    }
}

/// Emits the numeric key of a bounded failure-field or frame array.
fn push_php_index(output: &mut Vec<u8>, index: usize) {
    push_php_int(output, index as i64);
}

/// Converts one validated failure envelope into a safe PHP serialized field array.
pub(crate) fn failure_to_php_serialized(
    blob: ParallelTransferBlob,
) -> Result<ParallelTransferBlob, i32> {
    let failure = parse_failure(blob).ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(blob.len)
        .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
    output.extend_from_slice(b"a:7:{");
    push_php_index(&mut output, 0);
    push_php_int(&mut output, failure.kind as i64);
    push_php_index(&mut output, 1);
    push_php_optional_string(&mut output, failure.remote_class);
    push_php_index(&mut output, 2);
    push_php_string(&mut output, failure.message);
    push_php_index(&mut output, 3);
    push_php_int(&mut output, failure.code);
    push_php_index(&mut output, 4);
    push_php_optional_string(&mut output, failure.file);
    push_php_index(&mut output, 5);
    push_php_int(&mut output, failure.line as i64);
    push_php_index(&mut output, 6);
    output.extend_from_slice(b"a:");
    output.extend_from_slice(failure.frames.len().to_string().as_bytes());
    output.extend_from_slice(b":{");
    for (index, frame) in failure.frames.iter().enumerate() {
        push_php_index(&mut output, index);
        output.extend_from_slice(b"a:3:{");
        push_php_index(&mut output, 0);
        push_php_string(&mut output, frame.function);
        push_php_index(&mut output, 1);
        push_php_optional_string(&mut output, frame.file);
        push_php_index(&mut output, 2);
        push_php_int(&mut output, frame.line as i64);
        output.push(b'}');
    }
    output.extend_from_slice(b"}}");
    copy_transfer_payload(output.as_ptr(), output.len())
}

/// Returns a failure category only after validating its entire owned envelope.
pub(crate) fn failure_envelope_kind(blob: ParallelTransferBlob) -> Option<i32> {
    parse_failure(blob).map(|failure| failure.kind)
}

/// Builds an owned failure envelope with only a category/message and no remote object or frames.
pub(crate) fn simple_failure_blob(
    kind: i32,
    message: &[u8],
) -> Result<ParallelTransferBlob, i32> {
    let absent = ParallelBytes::EMPTY;
    let input = ParallelFailureInput {
        kind,
        remote_class: absent,
        message: ParallelBytes {
            ptr: message.as_ptr(),
            len: message.len(),
        },
        code: 0,
        file: absent,
        line: 0,
        frames: ptr::null(),
        frame_count: 0,
    };
    unsafe { encode_failure(&input) }
}

/// Converts an optional borrowed slice into a C view, using null/zero for absence.
fn bytes_view(value: Option<&[u8]>) -> ParallelBytes {
    value.map_or(ParallelBytes::EMPTY, |bytes| ParallelBytes {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    })
}

/// Copies caller failure fields into a new owned envelope while mapping encoding errors and panics.
///
/// # Safety
/// Nonnull input/output must reference readable/writable structs; all counted input slices and
/// frames must remain readable for their declared lengths during the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_failure_encode(
    input: *const ParallelFailureInput,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelTransferBlob::EMPTY) };
    if input.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    match catch_unwind(AssertUnwindSafe(|| unsafe { encode_failure(&*input) })) {
        Ok(Ok(blob)) => {
            unsafe { output.write(blob) };
            PARALLEL_TRANSFER_OK
        }
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Publishes borrowed failure fields only after full validation, clearing output on rejected input.
///
/// # Safety
/// The blob must remain readable and output writable; returned byte views borrow that live blob.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_failure_view(
    blob: ParallelTransferBlob,
    output: *mut ParallelFailureView,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelFailureView::EMPTY) };
    let Some(failure) = parse_failure(blob) else {
        return PARALLEL_TRANSFER_INVALID_ENVELOPE;
    };
    unsafe {
        output.write(ParallelFailureView {
            kind: failure.kind,
            remote_class: bytes_view(failure.remote_class),
            message: bytes_view(Some(failure.message)),
            code: failure.code,
            file: bytes_view(failure.file),
            line: failure.line,
            frame_count: failure.frames.len(),
        })
    };
    PARALLEL_TRANSFER_OK
}

/// Returns one bounded failure-frame view, distinguishing malformed envelopes from invalid indices.
///
/// # Safety
/// The blob must remain readable and output writable; returned byte views borrow that live blob.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_failure_frame(
    blob: ParallelTransferBlob,
    index: usize,
    output: *mut ParallelFailureFrameView,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelFailureFrameView::EMPTY) };
    let Some(failure) = parse_failure(blob) else {
        return PARALLEL_TRANSFER_INVALID_ENVELOPE;
    };
    let Some(frame) = failure.frames.get(index) else {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    };
    unsafe {
        output.write(ParallelFailureFrameView {
            function: bytes_view(Some(frame.function)),
            file: bytes_view(frame.file),
            line: frame.line,
        })
    };
    PARALLEL_TRANSFER_OK
}

#[cfg(test)]
mod tests;
