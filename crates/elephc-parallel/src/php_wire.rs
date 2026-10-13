//! Purpose:
//! Converts the safe PHP `serialize()` subset to and from Parallel's `EPV1` value envelope.
//!
//! Called from:
//! - Parent/worker codec wrappers after local-context PHP serialization.
//! - Future/input reconstruction before local-context PHP unserialization.
//!
//! Key details:
//! - Only null, bool, int, float, byte string, and recursively safe arrays are accepted.
//! - PHP objects, custom payloads, and `r:`/`R:` references fail closed.
//! - Cancellation stays an EPV-only capability and is reconstructed outside PHP unserialize.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
#[cfg(test)]
use std::sync::{Condvar, Mutex, OnceLock};
#[cfg(test)]
use std::time::{Duration, Instant};

use crate::failure::{
    encode_failure, failure_to_php_serialized, ParallelBytes, ParallelFailureFrameInput,
    ParallelFailureInput,
};
use crate::job::{
    elephc_parallel_job_complete, elephc_parallel_job_create, elephc_parallel_job_fail,
    elephc_parallel_job_input_copy, elephc_parallel_job_output_copy,
};
use crate::{
    copy_transfer_payload, elephc_parallel_value_begin_assoc,
    elephc_parallel_value_begin_indexed, elephc_parallel_value_read,
    elephc_parallel_value_reader_begin, elephc_parallel_value_write_bool,
    elephc_parallel_value_write_float, elephc_parallel_value_write_int,
    elephc_parallel_value_write_null, elephc_parallel_value_write_string,
    elephc_parallel_value_writer_finish, elephc_parallel_value_writer_free,
    elephc_parallel_value_writer_new, ParallelTransferBlob, ParallelValueView,
    PARALLEL_TRANSFER_ALLOCATION_FAILED, PARALLEL_TRANSFER_INVALID_ARGUMENT,
    PARALLEL_TRANSFER_INVALID_ENVELOPE, PARALLEL_TRANSFER_OK, PARALLEL_TRANSFER_PANIC,
    PARALLEL_VALUE_ASSOC_ARRAY, PARALLEL_VALUE_CANCELLATION, PARALLEL_VALUE_FALSE,
    PARALLEL_VALUE_FLOAT, PARALLEL_VALUE_INDEXED_ARRAY, PARALLEL_VALUE_INT,
    PARALLEL_VALUE_NULL, PARALLEL_VALUE_STRING, PARALLEL_VALUE_TRUE, TRANSFER_HEADER_LEN,
};

const MAX_DEPTH: usize = 256;

struct PreparedPhpBlob(ParallelTransferBlob);

impl Drop for PreparedPhpBlob {
    /// Releases the native transfer allocation when its thread-local owner is replaced or dropped.
    fn drop(&mut self) {
        unsafe { crate::release_transfer_blob(self.0) };
    }
}

thread_local! {
    /// One prepared PHP-wire blob per calling thread. Parent and worker contexts therefore never
    /// race a process-global output cell, and replacing/releasing the slot frees its native bytes.
    static PREPARED_PHP_BLOB: RefCell<Option<PreparedPhpBlob>> = const { RefCell::new(None) };
}

/// Test-only control point after a worker has decoded its PHP result into the owned EPV1
/// envelope, but before the job publishes that envelope as its terminal result.
///
/// The production path has no hook here. The test seam makes the cancellation-versus-transfer
/// ordering explicit without depending on thread scheduling luck.
#[cfg(test)]
#[derive(Default)]
struct CompletionTransferPause {
    job_id: Option<u64>,
    reached: bool,
    released: bool,
}

#[cfg(test)]
static COMPLETION_TRANSFER_PAUSE: OnceLock<(Mutex<CompletionTransferPause>, Condvar)> =
    OnceLock::new();

#[cfg(test)]
/// Initializes the shared test barrier for pausing one decoded result before publication.
fn completion_transfer_pause() -> &'static (Mutex<CompletionTransferPause>, Condvar) {
    COMPLETION_TRANSFER_PAUSE.get_or_init(|| {
        (
            Mutex::new(CompletionTransferPause::default()),
            Condvar::new(),
        )
    })
}

#[cfg(test)]
/// Blocks only the selected test job after decoding until the test releases its barrier.
fn pause_after_completion_transfer_decode(id: u64) {
    let (state, changed) = completion_transfer_pause();
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.job_id != Some(id) {
        return;
    }
    state.reached = true;
    changed.notify_all();
    while state.job_id == Some(id) && !state.released {
        state = changed
            .wait(state)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}

#[cfg(test)]
/// Selects a job and resets the result-publication barrier for its cancellation test.
fn arm_completion_transfer_pause(id: u64) {
    let (state, _) = completion_transfer_pause();
    *state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = CompletionTransferPause {
        job_id: Some(id),
        reached: false,
        released: false,
    };
}

#[cfg(test)]
/// Waits with a bounded deadline until the selected job reaches the publication barrier.
fn wait_for_completion_transfer_pause(id: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let (state, changed) = completion_transfer_pause();
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    while state.job_id == Some(id) && !state.reached {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("worker did not reach the completion-transfer pause");
        let (next, timeout) = changed
            .wait_timeout(state, remaining)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state = next;
        assert!(
            !timeout.timed_out(),
            "worker did not reach the completion-transfer pause"
        );
    }
    assert_eq!(state.job_id, Some(id));
}

#[cfg(test)]
/// Wakes the selected worker so its decoded result can become the terminal job outcome.
fn release_completion_transfer_pause(id: u64) {
    let (state, changed) = completion_transfer_pause();
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(state.job_id, Some(id));
    state.released = true;
    changed.notify_all();
}

enum PhpWireKey<'a> {
    Int(i64),
    String(&'a [u8]),
}

enum PhpWireValue<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(&'a [u8]),
    Array(Vec<(PhpWireKey<'a>, PhpWireValue<'a>)>),
}

struct Parser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Parser<'a> {
    /// Parses one safe serialized value and rejects any trailing bytes.
    fn parse(mut self) -> Option<PhpWireValue<'a>> {
        let value = self.value(0)?;
        (self.cursor == self.bytes.len()).then_some(value)
    }

    /// Decodes a supported scalar or array while enforcing the recursive depth limit.
    fn value(&mut self, depth: usize) -> Option<PhpWireValue<'a>> {
        if depth >= MAX_DEPTH {
            return None;
        }
        match self.take_byte()? {
            b'N' => {
                self.expect(b';')?;
                Some(PhpWireValue::Null)
            }
            b'b' => {
                self.expect(b':')?;
                let value = match self.take_byte()? {
                    b'0' => false,
                    b'1' => true,
                    _ => return None,
                };
                self.expect(b';')?;
                Some(PhpWireValue::Bool(value))
            }
            b'i' => {
                self.expect(b':')?;
                Some(PhpWireValue::Int(self.signed_integer(b';')?))
            }
            b'd' => {
                self.expect(b':')?;
                Some(PhpWireValue::Float(self.float()?))
            }
            b's' => {
                self.expect(b':')?;
                Some(PhpWireValue::String(self.string()?))
            }
            b'a' => {
                self.expect(b':')?;
                self.array(depth + 1)
            }
            _ => None,
        }
    }

    /// Accepts only integer and byte-string keys in a serialized PHP array.
    fn key(&mut self) -> Option<PhpWireKey<'a>> {
        match self.take_byte()? {
            b'i' => {
                self.expect(b':')?;
                Some(PhpWireKey::Int(self.signed_integer(b';')?))
            }
            b's' => {
                self.expect(b':')?;
                Some(PhpWireKey::String(self.string()?))
            }
            _ => None,
        }
    }

    /// Checks the declared array length, then reads its ordered key/value pairs and closing brace.
    fn array(&mut self, depth: usize) -> Option<PhpWireValue<'a>> {
        let count = self.unsigned_integer(b':')?;
        let count = usize::try_from(count).ok()?;
        self.expect(b'{')?;
        if count > self.bytes.len().saturating_sub(self.cursor) / 4 {
            return None;
        }
        let mut values = Vec::new();
        values.try_reserve_exact(count).ok()?;
        for _ in 0..count {
            let key = self.key()?;
            let value = self.value(depth)?;
            values.push((key, value));
        }
        self.expect(b'}')?;
        Some(PhpWireValue::Array(values))
    }

    /// Borrows exactly the declared byte-string payload and validates its closing delimiters.
    fn string(&mut self) -> Option<&'a [u8]> {
        let len = usize::try_from(self.unsigned_integer(b':')?).ok()?;
        self.expect(b'"')?;
        let end = self.cursor.checked_add(len)?;
        let value = self.bytes.get(self.cursor..end)?;
        self.cursor = end;
        self.expect(b'"')?;
        self.expect(b';')?;
        Some(value)
    }

    /// Parses a float token, including PHP's explicit infinity and NaN spellings.
    fn float(&mut self) -> Option<f64> {
        let token = self.until(b';')?;
        match token {
            b"INF" => Some(f64::INFINITY),
            b"-INF" => Some(f64::NEG_INFINITY),
            b"NAN" => Some(f64::NAN),
            _ => std::str::from_utf8(token).ok()?.parse().ok(),
        }
    }

    /// Parses a delimited signed integer with checked conversion to the wire's i64 carrier.
    fn signed_integer(&mut self, delimiter: u8) -> Option<i64> {
        let token = self.until(delimiter)?;
        std::str::from_utf8(token).ok()?.parse().ok()
    }

    /// Requires a nonempty decimal digit sequence for array counts and byte-string lengths.
    fn unsigned_integer(&mut self, delimiter: u8) -> Option<u64> {
        let token = self.until(delimiter)?;
        if token.is_empty() || token.iter().any(|byte| !byte.is_ascii_digit()) {
            return None;
        }
        std::str::from_utf8(token).ok()?.parse().ok()
    }

    /// Borrows bytes up to the next delimiter and advances the cursor past that delimiter.
    fn until(&mut self, delimiter: u8) -> Option<&'a [u8]> {
        let relative = self.bytes.get(self.cursor..)?.iter().position(|byte| *byte == delimiter)?;
        let end = self.cursor.checked_add(relative)?;
        let value = self.bytes.get(self.cursor..end)?;
        self.cursor = end + 1;
        Some(value)
    }

    /// Consumes one byte and rejects a delimiter that differs from the expected value.
    fn expect(&mut self, expected: u8) -> Option<()> {
        (self.take_byte()? == expected).then_some(())
    }

    /// Reads the next available byte without advancing beyond the input slice.
    fn take_byte(&mut self) -> Option<u8> {
        let value = *self.bytes.get(self.cursor)?;
        self.cursor += 1;
        Some(value)
    }
}

/// Encodes a parsed value into an exclusively owned writer.
///
/// # Safety
/// `writer` must be a live, uniquely accessible writer returned by
/// `elephc_parallel_value_writer_new` and not yet freed or consumed.
unsafe fn encode_value(writer: *mut crate::ParallelValueWriter, value: &PhpWireValue<'_>) -> i32 {
    // SAFETY: the caller upholds the writer contract for this entire recursive encoding pass;
    // string pointers borrow the parsed `value` only for the duration of each call.
    unsafe {
        match value {
            PhpWireValue::Null => elephc_parallel_value_write_null(writer),
            PhpWireValue::Bool(value) => {
                elephc_parallel_value_write_bool(writer, i32::from(*value))
            }
            PhpWireValue::Int(value) => elephc_parallel_value_write_int(writer, *value),
            PhpWireValue::Float(value) => {
                elephc_parallel_value_write_float(writer, value.to_bits())
            }
            PhpWireValue::String(value) => {
                elephc_parallel_value_write_string(writer, value.as_ptr(), value.len())
            }
            PhpWireValue::Array(values) => {
                let indexed = values.iter().enumerate().all(|(index, (key, _))| {
                    matches!(key, PhpWireKey::Int(value) if *value == index as i64)
                });
                let status = if indexed {
                    elephc_parallel_value_begin_indexed(writer, values.len())
                } else {
                    elephc_parallel_value_begin_assoc(writer, values.len())
                };
                if status != PARALLEL_TRANSFER_OK {
                    return status;
                }
                for (key, value) in values {
                    if !indexed {
                        let status = match key {
                            PhpWireKey::Int(value) => {
                                elephc_parallel_value_write_int(writer, *value)
                            }
                            PhpWireKey::String(value) => {
                                elephc_parallel_value_write_string(
                                    writer,
                                    value.as_ptr(),
                                    value.len(),
                                )
                            }
                        };
                        if status != PARALLEL_TRANSFER_OK {
                            return status;
                        }
                    }
                    let status = encode_value(writer, value);
                    if status != PARALLEL_TRANSFER_OK {
                        return status;
                    }
                }
                PARALLEL_TRANSFER_OK
            }
        }
    }
}

/// Parses the safe PHP subset into an owned EPV1 allocation, freeing an unfinished writer on error.
fn from_php_serialized(source: &[u8]) -> Result<ParallelTransferBlob, i32> {
    let value = Parser {
        bytes: source,
        cursor: 0,
    }
    .parse()
    .ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let writer = elephc_parallel_value_writer_new();
    if writer.is_null() {
        return Err(PARALLEL_TRANSFER_ALLOCATION_FAILED);
    }
    // SAFETY: `writer` was allocated above and remains exclusively owned here.
    let status = unsafe { encode_value(writer, &value) };
    if status != PARALLEL_TRANSFER_OK {
        unsafe { elephc_parallel_value_writer_free(writer) };
        return Err(status);
    }
    let mut output = ParallelTransferBlob::EMPTY;
    let status = unsafe { elephc_parallel_value_writer_finish(writer, &mut output) };
    if status == PARALLEL_TRANSFER_OK {
        Ok(output)
    } else {
        Err(status)
    }
}

/// Borrows consecutive zero-based array values, optionally requiring an exact field count.
fn indexed_values<'a>(
    value: &'a PhpWireValue<'a>,
    expected: Option<usize>,
) -> Option<Vec<&'a PhpWireValue<'a>>> {
    let PhpWireValue::Array(values) = value else {
        return None;
    };
    if expected.is_some_and(|expected| values.len() != expected) {
        return None;
    }
    values
        .iter()
        .enumerate()
        .map(|(index, (key, value))| {
            matches!(key, PhpWireKey::Int(key) if *key == index as i64).then_some(value)
        })
        .collect()
}

/// Distinguishes an absent null field from a present byte string, including an empty string.
fn php_optional_string<'a>(value: &'a PhpWireValue<'a>) -> Option<Option<&'a [u8]>> {
    match value {
        PhpWireValue::Null => Some(None),
        PhpWireValue::String(value) => Some(Some(*value)),
        _ => None,
    }
}

/// Builds a borrowed failure-field view with a null pointer only for an absent field.
fn parallel_bytes(value: Option<&[u8]>) -> ParallelBytes {
    value.map_or(
        ParallelBytes {
            ptr: ptr::null(),
            len: 0,
        },
        |value| ParallelBytes {
            ptr: value.as_ptr(),
            len: value.len(),
        },
    )
}

/// Validates the seven-field failure tuple and its frames before copying them into EPFL storage.
fn php_failure_blob(source: &[u8]) -> Result<ParallelTransferBlob, i32> {
    let value = Parser {
        bytes: source,
        cursor: 0,
    }
    .parse()
    .ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let fields = indexed_values(&value, Some(7)).ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let PhpWireValue::Int(kind) = fields[0] else {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    };
    let remote_class =
        php_optional_string(fields[1]).ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let PhpWireValue::String(message) = fields[2] else {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    };
    let PhpWireValue::Int(code) = fields[3] else {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    };
    let file = php_optional_string(fields[4]).ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let PhpWireValue::Int(line) = fields[5] else {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    };
    let line = u32::try_from(*line).map_err(|_| PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let frames = indexed_values(fields[6], None).ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
    let mut frame_inputs = Vec::new();
    frame_inputs
        .try_reserve_exact(frames.len())
        .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
    for frame in frames {
        let fields = indexed_values(frame, Some(3)).ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
        let PhpWireValue::String(function) = fields[0] else {
            return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
        };
        let file = php_optional_string(fields[1]).ok_or(PARALLEL_TRANSFER_INVALID_ENVELOPE)?;
        let PhpWireValue::Int(frame_line) = fields[2] else {
            return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
        };
        frame_inputs.push(ParallelFailureFrameInput {
            function: parallel_bytes(Some(function)),
            file: parallel_bytes(file),
            line: u32::try_from(*frame_line)
                .map_err(|_| PARALLEL_TRANSFER_INVALID_ENVELOPE)?,
        });
    }
    let input = ParallelFailureInput {
        kind: i32::try_from(*kind).map_err(|_| PARALLEL_TRANSFER_INVALID_ENVELOPE)?,
        remote_class: parallel_bytes(remote_class),
        message: parallel_bytes(Some(message)),
        code: *code,
        file: parallel_bytes(file),
        line,
        frames: frame_inputs.as_ptr(),
        frame_count: frame_inputs.len(),
    };
    unsafe { encode_failure(&input) }
}

/// Serializes one EPV1 subtree, preserving array keys and rejecting cancellation capabilities.
fn append_php_value(
    blob: ParallelTransferBlob,
    cursor: &mut usize,
    output: &mut Vec<u8>,
    depth: usize,
    key: bool,
) -> Result<(), i32> {
    if depth >= MAX_DEPTH {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    }
    let mut view = ParallelValueView::EMPTY;
    if unsafe { elephc_parallel_value_read(blob, cursor, &mut view) } != PARALLEL_TRANSFER_OK {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    }
    match view.tag {
        PARALLEL_VALUE_NULL if !key => output.extend_from_slice(b"N;"),
        PARALLEL_VALUE_FALSE if !key => output.extend_from_slice(b"b:0;"),
        PARALLEL_VALUE_TRUE if !key => output.extend_from_slice(b"b:1;"),
        PARALLEL_VALUE_INT => {
            output.extend_from_slice(b"i:");
            output.extend_from_slice(view.int_value.to_string().as_bytes());
            output.push(b';');
        }
        PARALLEL_VALUE_FLOAT if !key => {
            output.extend_from_slice(b"d:");
            if view.float_bits == f64::INFINITY.to_bits() {
                output.extend_from_slice(b"INF");
            } else if view.float_bits == f64::NEG_INFINITY.to_bits() {
                output.extend_from_slice(b"-INF");
            } else if f64::from_bits(view.float_bits).is_nan() {
                output.extend_from_slice(b"NAN");
            } else {
                output.extend_from_slice(f64::from_bits(view.float_bits).to_string().as_bytes());
            }
            output.push(b';');
        }
        PARALLEL_VALUE_STRING => {
            let bytes = unsafe { std::slice::from_raw_parts(view.bytes.ptr, view.bytes.len) };
            output.extend_from_slice(b"s:");
            output.extend_from_slice(bytes.len().to_string().as_bytes());
            output.extend_from_slice(b":\"");
            output.extend_from_slice(bytes);
            output.extend_from_slice(b"\";");
        }
        PARALLEL_VALUE_INDEXED_ARRAY if !key => {
            output.extend_from_slice(b"a:");
            output.extend_from_slice(view.count.to_string().as_bytes());
            output.extend_from_slice(b":{");
            for index in 0..view.count {
                output.extend_from_slice(b"i:");
                output.extend_from_slice(index.to_string().as_bytes());
                output.push(b';');
                append_php_value(blob, cursor, output, depth + 1, false)?;
            }
            output.push(b'}');
        }
        PARALLEL_VALUE_ASSOC_ARRAY if !key => {
            output.extend_from_slice(b"a:");
            output.extend_from_slice(view.count.to_string().as_bytes());
            output.extend_from_slice(b":{");
            for _ in 0..view.count {
                append_php_value(blob, cursor, output, depth + 1, true)?;
                append_php_value(blob, cursor, output, depth + 1, false)?;
            }
            output.push(b'}');
        }
        PARALLEL_VALUE_CANCELLATION => return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT),
        _ => return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE),
    }
    Ok(())
}

/// Validates an EPV1 value and copies its complete PHP serialization into a new transfer allocation.
fn to_php_serialized(blob: ParallelTransferBlob) -> Result<ParallelTransferBlob, i32> {
    let mut cursor = 0;
    if unsafe { elephc_parallel_value_reader_begin(blob, &mut cursor) } != PARALLEL_TRANSFER_OK {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(blob.len)
        .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
    append_php_value(blob, &mut cursor, &mut output, 0, false)?;
    if cursor != blob.len - TRANSFER_HEADER_LEN {
        return Err(PARALLEL_TRANSFER_INVALID_ENVELOPE);
    }
    copy_transfer_payload(output.as_ptr(), output.len())
}

/// Copies a safe serialized PHP value into EPV1 and clears the output on conversion failure.
///
/// # Safety
/// A nonempty source must be readable for `source_len` bytes. A nonnull output must be
/// writable for one blob and must not overlap the source. Successful output is caller-owned.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_value_from_php_serialized(
    source: *const u8,
    source_len: usize,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() || (source.is_null() && source_len != 0) {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelTransferBlob::EMPTY) };
    let source = if source_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(source, source_len) }
    };
    match catch_unwind(AssertUnwindSafe(|| from_php_serialized(source))) {
        Ok(Ok(blob)) => {
            unsafe { output.write(blob) };
            PARALLEL_TRANSFER_OK
        }
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Borrows an EPV1 allocation and returns a separately owned PHP-serialization allocation.
///
/// # Safety
/// The blob must reference readable storage for its declared length. A nonnull output must
/// be writable for one blob and disjoint from that storage. This does not consume the input.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_value_to_php_serialized(
    blob: ParallelTransferBlob,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelTransferBlob::EMPTY) };
    match catch_unwind(AssertUnwindSafe(|| to_php_serialized(blob))) {
        Ok(Ok(serialized)) => {
            unsafe { output.write(serialized) };
            PARALLEL_TRANSFER_OK
        }
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Creates a native job from bytes produced by PHP's local-context `serialize()`.
/// Ownership of the intermediate `EPV1` envelope transfers only when job creation succeeds.
///
/// # Safety
/// A nonempty source must remain readable for `source_len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_create_php_serialized(
    source: *const u8,
    source_len: usize,
) -> u64 {
    if source.is_null() && source_len != 0 {
        return 0;
    }
    let source = if source_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(source, source_len) }
    };
    match catch_unwind(AssertUnwindSafe(|| {
        let value = from_php_serialized(source)?;
        let id = elephc_parallel_job_create(value);
        if id == 0 {
            unsafe { crate::release_transfer_blob(value) };
            return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
        }
        Ok(id)
    })) {
        Ok(Ok(id)) => id,
        Ok(Err(_)) | Err(_) => 0,
    }
}

/// Copies a job's input into bytes accepted by the worker context's `unserialize()`.
///
/// # Safety
/// A nonnull output must be writable for one blob; a successful copy is caller-owned.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_input_php_serialized(
    id: u64,
    output: *mut ParallelTransferBlob,
) -> i32 {
    convert_job_value_to_php_serialized(id, 0, output)
}

/// Completes a running job from bytes produced by the worker context's `serialize()`.
///
/// # Safety
/// A nonempty source must remain readable for `source_len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_complete_php_serialized(
    id: u64,
    source: *const u8,
    source_len: usize,
) -> i32 {
    if source.is_null() && source_len != 0 {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    let source = if source_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(source, source_len) }
    };
    match catch_unwind(AssertUnwindSafe(|| {
        let value = from_php_serialized(source)?;
        #[cfg(test)]
        pause_after_completion_transfer_decode(id);
        let status = elephc_parallel_job_complete(id, value);
        if status != 1 {
            unsafe { crate::release_transfer_blob(value) };
        }
        Ok(status)
    })) {
        Ok(Ok(status)) => status,
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Fails a running job from a safe serialized field array produced in the worker context.
///
/// # Safety
/// A nonempty source must remain readable for `source_len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_fail_php_serialized(
    id: u64,
    source: *const u8,
    source_len: usize,
) -> i32 {
    if source.is_null() && source_len != 0 {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    let source = if source_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(source, source_len) }
    };
    match catch_unwind(AssertUnwindSafe(|| {
        let failure = php_failure_blob(source)?;
        let status = elephc_parallel_job_fail(id, failure);
        if status != 1 {
            unsafe { crate::release_transfer_blob(failure) };
        }
        Ok(status)
    })) {
        Ok(Ok(status)) => status,
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Copies a completed job's result into bytes accepted by the parent context's `unserialize()`.
///
/// # Safety
/// A nonnull output must be writable for one blob; a successful copy is caller-owned.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_result_php_serialized(
    id: u64,
    output: *mut ParallelTransferBlob,
) -> i32 {
    convert_job_value_to_php_serialized(id, 1, output)
}

/// Converts an input (`which=0`) or completed result (`which=1`) copy to PHP wire bytes.
///
/// # Safety
/// A nonnull output must be writable for one blob; conversion releases the temporary EPV1 copy.
unsafe fn convert_job_value_to_php_serialized(
    id: u64,
    which: i32,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe { output.write(ParallelTransferBlob::EMPTY) };
    match catch_unwind(AssertUnwindSafe(|| {
        let mut value = ParallelTransferBlob::EMPTY;
        let status = if which == 0 {
            unsafe { elephc_parallel_job_input_copy(id, &mut value) }
        } else {
            unsafe { elephc_parallel_job_output_copy(id, 1, &mut value) }
        };
        if status != PARALLEL_TRANSFER_OK {
            return Err(status);
        }
        if value.ptr.is_null() {
            return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
        }
        let converted = to_php_serialized(value);
        unsafe { crate::release_transfer_blob(value) };
        converted
    })) {
        Ok(Ok(serialized)) => {
            unsafe { output.write(serialized) };
            PARALLEL_TRANSFER_OK
        }
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Prepares a worker's input in the calling thread's private PHP-wire slot.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_input_php_prepare(id: u64) -> i32 {
    prepare_job_value(id, 0)
}

/// Prepares a completed result in the calling thread's private PHP-wire slot.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_result_php_prepare(id: u64) -> i32 {
    prepare_job_value(id, 1)
}

/// Prepares a failed job's data-only envelope as a safe PHP serialized field array.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_failure_php_prepare(id: u64) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| {
        PREPARED_PHP_BLOB.with(|slot| slot.borrow_mut().take());
        let mut failure = ParallelTransferBlob::EMPTY;
        let status = unsafe { elephc_parallel_job_output_copy(id, 2, &mut failure) };
        if status != PARALLEL_TRANSFER_OK {
            return status;
        }
        if failure.ptr.is_null() {
            return PARALLEL_TRANSFER_INVALID_ARGUMENT;
        }
        let serialized = failure_to_php_serialized(failure);
        unsafe { crate::release_transfer_blob(failure) };
        let Ok(serialized) = serialized else {
            return PARALLEL_TRANSFER_INVALID_ENVELOPE;
        };
        PREPARED_PHP_BLOB.with(|slot| {
            *slot.borrow_mut() = Some(PreparedPhpBlob(serialized));
        });
        PARALLEL_TRANSFER_OK
    })) {
        Ok(status) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Replaces the calling thread's prepared slot with an owned input or result serialization.
fn prepare_job_value(id: u64, which: i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| {
        PREPARED_PHP_BLOB.with(|slot| slot.borrow_mut().take());
        let mut output = ParallelTransferBlob::EMPTY;
        let status = unsafe { convert_job_value_to_php_serialized(id, which, &mut output) };
        if status != PARALLEL_TRANSFER_OK {
            return status;
        }
        PREPARED_PHP_BLOB.with(|slot| {
            *slot.borrow_mut() = Some(PreparedPhpBlob(output));
        });
        PARALLEL_TRANSFER_OK
    })) {
        Ok(status) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Returns the exact byte pointer of the calling thread's prepared PHP serialization.
/// The borrowed pointer expires on the next preparation, explicit release or thread teardown.
#[no_mangle]
pub extern "C" fn elephc_parallel_php_blob_ptr() -> *const u8 {
    catch_unwind(AssertUnwindSafe(|| {
        PREPARED_PHP_BLOB.with(|slot| {
            slot.borrow()
                .as_ref()
                .map(|blob| unsafe { blob.0.ptr.add(TRANSFER_HEADER_LEN) }.cast_const())
                .unwrap_or(ptr::null())
        })
    }))
    .unwrap_or(ptr::null())
}

/// Returns the exact byte length of the calling thread's prepared PHP serialization.
#[no_mangle]
pub extern "C" fn elephc_parallel_php_blob_len() -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        PREPARED_PHP_BLOB.with(|slot| {
            slot.borrow()
                .as_ref()
                .map(|blob| blob.0.len - TRANSFER_HEADER_LEN)
                .unwrap_or(0)
        })
    }))
    .unwrap_or(0)
}

/// Releases the calling thread's prepared PHP serialization. Idempotent.
#[no_mangle]
pub extern "C" fn elephc_parallel_php_blob_release() {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        PREPARED_PHP_BLOB.with(|slot| slot.borrow_mut().take());
    }));
}

/// Allocates a native byte buffer for one generated PHP-to-bridge copy.
#[no_mangle]
pub extern "C" fn elephc_parallel_php_buffer_alloc(len: usize) -> *mut u8 {
    catch_unwind(AssertUnwindSafe(|| {
        let mut bytes = Vec::<u8>::new();
        if bytes.try_reserve_exact(len).is_err() {
            return ptr::null_mut();
        }
        bytes.resize(len, 0);
        let mut bytes = bytes.into_boxed_slice();
        let pointer = bytes.as_mut_ptr();
        std::mem::forget(bytes);
        pointer
    }))
    .unwrap_or(ptr::null_mut())
}

/// Frees a native byte buffer returned by `elephc_parallel_php_buffer_alloc`.
///
/// # Safety
/// A nonnull pointer must be an unreleased allocation from that allocator with exactly `len`
/// bytes, and no access to the buffer may outlive this call. Null is accepted as a no-op.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_php_buffer_free(pointer: *mut u8, len: usize) {
    if pointer.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let slice = ptr::slice_from_raw_parts_mut(pointer, len);
        drop(unsafe { Box::from_raw(slice) });
    }));
}

#[cfg(test)]
mod tests;
