//! Purpose:
//! Owns thread-safe native job/Future state independently of the executor policy.
//!
//! Called from:
//! - Parallel submission, worker completion, cancellation, and parent-side join paths.
//!
//! Key details:
//! - IDs are monotonic and never reused, so a stale Future cannot address a new job.
//! - Input/result/failure blobs remain native-owned until terminal job release.
//! - Wait uses a condition variable; running cancellation is cooperative and observable.

use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use crate::{
    copy_transfer_payload, failure_envelope_kind, release_transfer_blob, simple_failure_blob,
    value_envelope_is_valid, ParallelTransferBlob, PARALLEL_FAILURE_ARENA_EXHAUSTED,
    PARALLEL_TRANSFER_INVALID_ARGUMENT, PARALLEL_TRANSFER_OK, TRANSFER_HEADER_LEN,
};

pub const PARALLEL_JOB_QUEUED: i32 = 1;
pub const PARALLEL_JOB_RUNNING: i32 = 2;
pub const PARALLEL_JOB_COMPLETED: i32 = 3;
pub const PARALLEL_JOB_FAILED: i32 = 4;
pub const PARALLEL_JOB_CANCELLED: i32 = 5;
pub const PARALLEL_JOB_ERROR: i32 = -1;

struct OwnedBlob(ParallelTransferBlob);

unsafe impl Send for OwnedBlob {}
unsafe impl Sync for OwnedBlob {}

impl Drop for OwnedBlob {
    /// Releases the native transfer allocation exactly when its owning record drops the blob.
    fn drop(&mut self) {
        unsafe { release_transfer_blob(self.0) };
    }
}

struct JobData {
    phase: i32,
    submitted: bool,
    cancellation_requested: bool,
    input: Option<OwnedBlob>,
    result: Option<OwnedBlob>,
    failure_kind: i32,
    failure_observed: bool,
    failure: Option<OwnedBlob>,
    emergency_failure: Option<OwnedBlob>,
}

struct JobRecord {
    data: Mutex<JobData>,
    changed: Condvar,
}

struct CompletionSignal {
    generation: Mutex<u64>,
    changed: Condvar,
}

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);
static JOBS: OnceLock<Mutex<HashMap<u64, Arc<JobRecord>>>> = OnceLock::new();
static COMPLETIONS: OnceLock<CompletionSignal> = OnceLock::new();

/// Initializes the process-wide generation/condition-variable pair for terminal transitions.
fn completions() -> &'static CompletionSignal {
    COMPLETIONS.get_or_init(|| CompletionSignal {
        generation: Mutex::new(0),
        changed: Condvar::new(),
    })
}

/// Advances the completion generation under its mutex and wakes every group-level waiter.
fn notify_completion() {
    let signal = completions();
    let mut generation = signal
        .generation
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *generation = generation.wrapping_add(1);
    signal.changed.notify_all();
    #[cfg(unix)]
    crate::completion_wake::notify_waiters();
}

/// Returns the process-wide terminal-transition generation for race-free group waiting.
#[no_mangle]
pub extern "C" fn elephc_parallel_completion_generation() -> u64 {
    completions()
        .generation
        .lock()
        .map(|generation| *generation)
        .unwrap_or(u64::MAX)
}

/// Blocks until any job makes a terminal transition after `observed`.
#[no_mangle]
pub extern "C" fn elephc_parallel_completion_wait(observed: u64) -> u64 {
    let signal = completions();
    let Ok(mut generation) = signal.generation.lock() else {
        return u64::MAX;
    };
    while *generation == observed {
        let Ok(next) = signal.changed.wait(generation) else {
            return u64::MAX;
        };
        generation = next;
    }
    *generation
}

/// Initializes the registry that owns job records until terminal release.
fn jobs() -> &'static Mutex<HashMap<u64, Arc<JobRecord>>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clones a record lease under the registry lock so later state operations cannot use freed storage.
fn job(id: u64) -> Option<Arc<JobRecord>> {
    jobs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&id)
        .cloned()
}

/// Allocates a never-reused nonzero id, failing rather than wrapping after exhaustion.
fn next_job_id() -> Option<u64> {
    NEXT_JOB_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            (current != 0 && current != u64::MAX).then_some(current + 1)
        })
        .ok()
}

/// Identifies states ready for joining/release, including cancellation before worker start.
fn terminal(phase: i32) -> bool {
    matches!(
        phase,
        PARALLEL_JOB_COMPLETED | PARALLEL_JOB_FAILED | PARALLEL_JOB_CANCELLED
    )
}

/// Creates one queued job and transfers ownership of `input` on success.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_create(input: ParallelTransferBlob) -> u64 {
    if !value_envelope_is_valid(input) {
        return 0;
    }
    let Some(id) = next_job_id() else {
        return 0;
    };
    let Ok(emergency_failure) = simple_failure_blob(
        PARALLEL_FAILURE_ARENA_EXHAUSTED,
        b"Parallel failure envelope allocation failed",
    ) else {
        return 0;
    };
    let mut jobs = jobs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let record = Arc::new(JobRecord {
        data: Mutex::new(JobData {
            phase: PARALLEL_JOB_QUEUED,
            submitted: false,
            cancellation_requested: false,
            input: Some(OwnedBlob(input)),
            result: None,
            failure_kind: 0,
            failure_observed: false,
            failure: None,
            emergency_failure: Some(OwnedBlob(emergency_failure)),
        }),
        changed: Condvar::new(),
    });
    jobs.insert(id, record);
    id
}

/// Transitions only a queued job to Running, leaving cancelled/terminal records unchanged.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_mark_running(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(mut data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    if data.phase != PARALLEL_JOB_QUEUED {
        return 0;
    }
    data.phase = PARALLEL_JOB_RUNNING;
    1
}

/// Cancels queued jobs immediately or requests cooperative cancellation from a running worker.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_cancel(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(mut data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    match data.phase {
        PARALLEL_JOB_QUEUED => {
            data.phase = PARALLEL_JOB_CANCELLED;
            data.cancellation_requested = true;
            job.changed.notify_all();
            notify_completion();
            1
        }
        PARALLEL_JOB_RUNNING => {
            data.cancellation_requested = true;
            1
        }
        _ => 0,
    }
}

/// Reads the cancellation request flag without treating a running request as terminal completion.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_cancellation_requested(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let status = match job.data.lock() {
        Ok(data) => i32::from(data.cancellation_requested),
        Err(_) => PARALLEL_JOB_ERROR,
    };
    status
}

/// Publishes a structurally valid value result, transferring its blob only on a successful transition.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_complete(
    id: u64,
    result: ParallelTransferBlob,
) -> i32 {
    finish_job(id, PARALLEL_JOB_COMPLETED, 0, result)
}

/// Publishes a validated failure envelope/kind, transferring ownership only when the running job settles.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_fail(
    id: u64,
    failure: ParallelTransferBlob,
) -> i32 {
    let Some(failure_kind) = failure_envelope_kind(failure) else {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    };
    finish_job(id, PARALLEL_JOB_FAILED, failure_kind, failure)
}

/// Validates and atomically settles a running record, then wakes job/group waiters.
/// Rejected transitions leave ownership of the incoming blob with its caller.
fn finish_job(id: u64, phase: i32, failure_kind: i32, blob: ParallelTransferBlob) -> i32 {
    if (phase == PARALLEL_JOB_COMPLETED && !value_envelope_is_valid(blob))
        || (phase == PARALLEL_JOB_FAILED && failure_envelope_kind(blob).is_none())
    {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(mut data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    if data.phase != PARALLEL_JOB_RUNNING {
        return 0;
    }
    if phase == PARALLEL_JOB_COMPLETED {
        data.result = Some(OwnedBlob(blob));
    } else {
        data.failure_kind = failure_kind;
        data.failure = Some(OwnedBlob(blob));
    }
    data.emergency_failure = None;
    data.phase = phase;
    job.changed.notify_all();
    notify_completion();
    1
}

/// Reads the synchronized phase or reports an invalid id/poisoned record.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_phase(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let phase = match job.data.lock() {
        Ok(data) => data.phase,
        Err(_) => PARALLEL_JOB_ERROR,
    };
    phase
}

/// Waits for a terminal phase and joins the worker before returning that phase to the parent.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_wait(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(mut data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    while !terminal(data.phase) {
        let Ok(next) = job.changed.wait(data) else {
            return PARALLEL_JOB_ERROR;
        };
        data = next;
    }
    let phase = data.phase;
    drop(data);
    if !crate::executor::join_worker(id) {
        return PARALLEL_JOB_ERROR;
    }
    phase
}

/// Returns a borrowed input payload while the job is queued or running.
///
/// # Safety
/// Nonnull output slots must be writable. Returned bytes remain owned by the job;
/// the caller must prevent job release while it accesses that borrowed payload.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_input(
    id: u64,
    payload_ptr: *mut *const u8,
    payload_len: *mut usize,
) -> i32 {
    if payload_ptr.is_null() || payload_len.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    payload_ptr.write(ptr::null());
    payload_len.write(0);
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    if terminal(data.phase) {
        return 0;
    }
    let Some(blob) = data.input.as_ref() else {
        return 0;
    };
    payload_ptr.write(blob.0.ptr.add(TRANSFER_HEADER_LEN));
    payload_len.write(blob.0.len - TRANSFER_HEADER_LEN);
    PARALLEL_TRANSFER_OK
}

/// Copies the queued/running input into worker-owned storage suitable for value decoding.
///
/// # Safety
/// Nonnull output must be writable for one blob; a published copy is independently owned
/// and must be released by its receiving caller.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_input_copy(
    id: u64,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe {
        output.write(ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        })
    };
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    if terminal(data.phase) {
        return 0;
    }
    let Some(blob) = data.input.as_ref() else {
        return 0;
    };
    let source = unsafe { blob.0.ptr.add(TRANSFER_HEADER_LEN) };
    let source_len = blob.0.len - TRANSFER_HEADER_LEN;
    match copy_transfer_payload(source, source_len) {
        Ok(copy) => {
            unsafe { output.write(copy) };
            PARALLEL_TRANSFER_OK
        }
        Err(status) => status,
    }
}

/// Copies the terminal result (`which=1`) or failure (`which=2`) into caller-owned storage.
///
/// # Safety
/// Nonnull output must be writable for one blob; successful copies do not borrow the
/// job's allocation and must be released independently by the caller.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_job_output_copy(
    id: u64,
    which: i32,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    output.write(ParallelTransferBlob {
        ptr: ptr::null_mut(),
        len: 0,
    });
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    let blob = match which {
        1 if data.phase == PARALLEL_JOB_COMPLETED => data.result.as_ref(),
        2 if data.phase == PARALLEL_JOB_FAILED => data.failure.as_ref(),
        1 | 2 => return 0,
        _ => return PARALLEL_TRANSFER_INVALID_ARGUMENT,
    };
    let Some(blob) = blob else {
        return 0;
    };
    let source = blob.0.ptr.add(TRANSFER_HEADER_LEN);
    let source_len = blob.0.len - TRANSFER_HEADER_LEN;
    match copy_transfer_payload(source, source_len) {
        Ok(copy) => {
            output.write(copy);
            PARALLEL_TRANSFER_OK
        }
        Err(status) => status,
    }
}

/// Reads a record's retained failure category without consuming its envelope or marking it observed.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_failure_kind(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let kind = match job.data.lock() {
        Ok(data) => data.failure_kind,
        Err(_) => PARALLEL_JOB_ERROR,
    };
    kind
}

/// Marks a failed job as observed by its corresponding Future join.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_observe_failure(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let Ok(mut data) = job.data.lock() else {
        return PARALLEL_JOB_ERROR;
    };
    if data.phase != PARALLEL_JOB_FAILED {
        return 0;
    }
    data.failure_observed = true;
    1
}

/// Reports whether a failed job was already observed through Future::join().
#[no_mangle]
pub extern "C" fn elephc_parallel_job_failure_observed(id: u64) -> i32 {
    let Some(job) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    let observed = match job.data.lock() {
        Ok(data) if data.phase == PARALLEL_JOB_FAILED => i32::from(data.failure_observed),
        Ok(_) => 0,
        Err(_) => PARALLEL_JOB_ERROR,
    };
    observed
}

/// Removes and frees a terminal job. Live jobs remain registered for structured ownership.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_release(id: u64) -> i32 {
    let Some(record) = job(id) else {
        return PARALLEL_JOB_ERROR;
    };
    if !record
        .data
        .lock()
        .map(|data| terminal(data.phase))
        .unwrap_or(false)
    {
        return 0;
    }
    if !crate::executor::join_worker(id) {
        return PARALLEL_JOB_ERROR;
    }
    crate::executor::forget_worker(id);
    let mut jobs = jobs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(current) = jobs.get(&id) else {
        return PARALLEL_JOB_ERROR;
    };
    if !Arc::ptr_eq(current, &record) {
        return PARALLEL_JOB_ERROR;
    }
    jobs.remove(&id);
    1
}

/// Grants one executor submission for a queued job, preventing duplicate native worker starts.
pub(crate) fn reserve_submission(id: u64) -> bool {
    let Some(job) = job(id) else {
        return false;
    };
    let Ok(mut data) = job.data.lock() else {
        return false;
    };
    if data.phase != PARALLEL_JOB_QUEUED || data.submitted {
        return false;
    }
    data.submitted = true;
    true
}

/// Rolls back a failed submission reservation only while the record is still queued.
pub(crate) fn unreserve_submission(id: u64) {
    let Some(job) = job(id) else {
        return;
    };
    if let Ok(mut data) = job.data.lock() {
        if data.phase == PARALLEL_JOB_QUEUED {
            data.submitted = false;
        }
    };
}

/// Terminalizes a cancellation request after its running worker has actually exited.
pub(crate) fn cancel_running_after_worker_exit(id: u64) -> bool {
    let Some(job) = job(id) else {
        return false;
    };
    let Ok(mut data) = job.data.lock() else {
        return false;
    };
    if data.phase != PARALLEL_JOB_RUNNING || !data.cancellation_requested {
        return false;
    }
    data.phase = PARALLEL_JOB_CANCELLED;
    job.changed.notify_all();
    notify_completion();
    true
}

/// Settles a running job with its preallocated failure envelope when ordinary failure allocation fails.
pub(crate) fn fail_with_emergency_envelope(id: u64) -> bool {
    let Some(job) = job(id) else {
        return false;
    };
    let Ok(mut data) = job.data.lock() else {
        return false;
    };
    if data.phase != PARALLEL_JOB_RUNNING {
        return false;
    }
    let Some(failure) = data.emergency_failure.take() else {
        return false;
    };
    data.failure_kind = PARALLEL_FAILURE_ARENA_EXHAUSTED;
    data.failure = Some(failure);
    data.phase = PARALLEL_JOB_FAILED;
    job.changed.notify_all();
    notify_completion();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        elephc_parallel_failure_encode, elephc_parallel_failure_view,
        elephc_parallel_transfer_copy, elephc_parallel_value_read,
        elephc_parallel_value_reader_begin,
        elephc_parallel_value_write_string, elephc_parallel_value_writer_finish,
        elephc_parallel_value_writer_new, ParallelBytes, ParallelFailureInput,
        ParallelFailureView, ParallelTransferBlob, ParallelValueView,
        PARALLEL_FAILURE_CONTEXT_UNAVAILABLE,
    };

    /// Creates an owned EPV string envelope for job-state fixtures.
    fn blob(bytes: &[u8]) -> ParallelTransferBlob {
        let writer = elephc_parallel_value_writer_new();
        assert!(!writer.is_null());
        assert_eq!(
            unsafe { elephc_parallel_value_write_string(writer, bytes.as_ptr(), bytes.len()) },
            PARALLEL_TRANSFER_OK
        );
        let mut blob = ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_value_writer_finish(writer, &mut blob) },
            PARALLEL_TRANSFER_OK
        );
        blob
    }

    /// Creates an outer transfer blob without EPV validation for malformed-input controls.
    fn raw_blob(bytes: &[u8]) -> ParallelTransferBlob {
        let mut blob = ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_transfer_copy(bytes.as_ptr(), bytes.len(), &mut blob) },
            PARALLEL_TRANSFER_OK
        );
        blob
    }

    /// Encodes an owned minimal EPFL fixture with the requested category and message.
    fn failure_blob(kind: i32, message: &[u8]) -> ParallelTransferBlob {
        let absent = ParallelBytes {
            ptr: ptr::null(),
            len: 0,
        };
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
        let mut blob = ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_failure_encode(&input, &mut blob) },
            PARALLEL_TRANSFER_OK
        );
        blob
    }

    /// Copies and decodes a live record's input without retaining a borrowed job pointer.
    fn input(id: u64) -> Vec<u8> {
        let mut copy = ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_job_input_copy(id, &mut copy) },
            PARALLEL_TRANSFER_OK
        );
        decode_string(copy)
    }

    /// Copies and decodes a selected terminal value while leaving its cached job allocation intact.
    fn output(id: u64, which: i32) -> Vec<u8> {
        let mut copy = ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_job_output_copy(id, which, &mut copy) },
            PARALLEL_TRANSFER_OK
        );
        decode_string(copy)
    }

    /// Copies a string view into test-owned bytes before freeing its source envelope.
    fn decode_string(blob: ParallelTransferBlob) -> Vec<u8> {
        let mut cursor = 0;
        assert_eq!(
            unsafe { elephc_parallel_value_reader_begin(blob, &mut cursor) },
            PARALLEL_TRANSFER_OK
        );
        let mut view = ParallelValueView {
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
        assert_eq!(
            unsafe { elephc_parallel_value_read(blob, &mut cursor, &mut view) },
            PARALLEL_TRANSFER_OK
        );
        let bytes = unsafe { std::slice::from_raw_parts(view.bytes.ptr, view.bytes.len) }.to_vec();
        unsafe { crate::elephc_parallel_transfer_free(blob) };
        bytes
    }

    /// Completed jobs retain input and repeatable copied results until terminal release removes the id.
    #[test]
    fn completed_job_keeps_input_and_result_until_release() {
        let id = elephc_parallel_job_create(blob(b"input"));
        assert_ne!(id, 0);
        assert_eq!(elephc_parallel_job_phase(id), PARALLEL_JOB_QUEUED);
        assert_eq!(elephc_parallel_job_release(id), 0);
        assert_eq!(input(id), b"input");
        assert_eq!(elephc_parallel_job_mark_running(id), 1);
        assert_eq!(elephc_parallel_job_complete(id, blob(b"result")), 1);
        assert_eq!(elephc_parallel_job_wait(id), PARALLEL_JOB_COMPLETED);
        assert_eq!(output(id, 1), b"result");
        assert_eq!(output(id, 1), b"result");
        assert_eq!(elephc_parallel_job_release(id), 1);
        assert_eq!(elephc_parallel_job_phase(id), PARALLEL_JOB_ERROR);
    }

    /// Real terminal transitions wake each registered coordinator through the exported ABI.
    #[cfg(unix)]
    #[test]
    fn completion_wake_terminal_transitions_notify_subscribers() {
        use crate::completion_wake::*;
        for phase in [PARALLEL_JOB_COMPLETED, PARALLEL_JOB_FAILED, PARALLEL_JOB_CANCELLED] {
            let left = elephc_parallel_completion_wake_create();
            let right = elephc_parallel_completion_wake_create();
            assert_ne!(left, 0);
            assert_ne!(right, 0);
            let id = elephc_parallel_job_create(blob(b"input"));
            assert_ne!(id, 0);
            if phase == PARALLEL_JOB_CANCELLED {
                assert_eq!(elephc_parallel_job_cancel(id), 1);
            } else {
                assert_eq!(elephc_parallel_job_mark_running(id), 1);
                if phase == PARALLEL_JOB_COMPLETED {
                    assert_eq!(elephc_parallel_job_complete(id, blob(b"result")), 1);
                } else {
                    assert_eq!(elephc_parallel_job_fail(id,
                        failure_blob(PARALLEL_FAILURE_CONTEXT_UNAVAILABLE, b"failure")), 1);
                }
            }
            assert_eq!(elephc_parallel_job_phase(id), phase);
            assert!(elephc_parallel_completion_wake_fd(left) >= 0);
            assert_eq!(elephc_parallel_completion_wake_drain(left), 1);
            assert_eq!(elephc_parallel_completion_wake_drain(right), 1);
            assert_eq!(elephc_parallel_job_release(id), 1);
            assert_eq!(elephc_parallel_completion_wake_release(left), 1);
            assert_eq!(elephc_parallel_completion_wake_release(right), 1);
        }
    }

    /// Raw non-EPV payloads cannot become job inputs/results and remain caller-owned on rejection.
    #[test]
    fn jobs_reject_raw_transfer_blobs_for_php_values() {
        let invalid_input = raw_blob(b"not EPV1");
        assert_eq!(elephc_parallel_job_create(invalid_input), 0);
        unsafe { release_transfer_blob(invalid_input) };

        let id = elephc_parallel_job_create(blob(b"input"));
        assert_eq!(elephc_parallel_job_mark_running(id), 1);
        let invalid_result = raw_blob(b"not EPV1");
        assert_eq!(
            elephc_parallel_job_complete(id, invalid_result),
            PARALLEL_TRANSFER_INVALID_ARGUMENT
        );
        unsafe { release_transfer_blob(invalid_result) };
        assert_eq!(elephc_parallel_job_complete(id, blob(b"result")), 1);
        assert_eq!(elephc_parallel_job_release(id), 1);
    }

    /// Queued cancellation is terminal, whereas a running request remains cooperative until exit.
    #[test]
    fn queued_and_running_cancellation_have_distinct_lifecycles() {
        let queued = elephc_parallel_job_create(blob(b"queued"));
        assert_eq!(elephc_parallel_job_cancel(queued), 1);
        assert_eq!(elephc_parallel_job_wait(queued), PARALLEL_JOB_CANCELLED);
        assert_eq!(elephc_parallel_job_release(queued), 1);

        let running = elephc_parallel_job_create(blob(b"running"));
        assert_eq!(elephc_parallel_job_mark_running(running), 1);
        assert_eq!(elephc_parallel_job_cancel(running), 1);
        assert_eq!(elephc_parallel_job_cancellation_requested(running), 1);
        assert_eq!(elephc_parallel_job_phase(running), PARALLEL_JOB_RUNNING);
        assert_eq!(elephc_parallel_job_complete(running, blob(b"done")), 1);
        assert_eq!(elephc_parallel_job_release(running), 1);
    }

    /// Failure kind/data remain copyable and observation is explicit rather than consuming the envelope.
    #[test]
    fn failure_kind_and_payload_are_retained() {
        let id = elephc_parallel_job_create(blob(b"input"));
        assert_eq!(elephc_parallel_job_mark_running(id), 1);
        let invalid = raw_blob(b"not a failure envelope");
        assert_eq!(
            elephc_parallel_job_fail(id, invalid),
            PARALLEL_TRANSFER_INVALID_ARGUMENT
        );
        unsafe { release_transfer_blob(invalid) };
        assert_eq!(
            elephc_parallel_job_fail(
                id,
                failure_blob(PARALLEL_FAILURE_CONTEXT_UNAVAILABLE, b"failure")
            ),
            1
        );
        assert_eq!(elephc_parallel_job_wait(id), PARALLEL_JOB_FAILED);
        assert_eq!(
            elephc_parallel_job_failure_kind(id),
            PARALLEL_FAILURE_CONTEXT_UNAVAILABLE
        );
        assert_eq!(elephc_parallel_job_failure_observed(id), 0);
        assert_eq!(elephc_parallel_job_observe_failure(id), 1);
        assert_eq!(elephc_parallel_job_failure_observed(id), 1);
        let mut copy = ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_job_output_copy(id, 2, &mut copy) },
            PARALLEL_TRANSFER_OK
        );
        let mut view = ParallelFailureView {
            kind: 0,
            remote_class: ParallelBytes {
                ptr: ptr::null(),
                len: 0,
            },
            message: ParallelBytes {
                ptr: ptr::null(),
                len: 0,
            },
            code: 0,
            file: ParallelBytes {
                ptr: ptr::null(),
                len: 0,
            },
            line: 0,
            frame_count: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_failure_view(copy, &mut view) },
            PARALLEL_TRANSFER_OK
        );
        assert_eq!(
            unsafe { std::slice::from_raw_parts(view.message.ptr, view.message.len) },
            b"failure"
        );
        unsafe { crate::elephc_parallel_transfer_free(copy) };
        assert_eq!(elephc_parallel_job_release(id), 1);
    }

    /// A preallocated failure envelope lets allocation failure settle instead of stranding a running job.
    #[test]
    fn emergency_failure_envelope_prevents_a_running_job_from_stalling() {
        let id = elephc_parallel_job_create(blob(b"input"));
        assert_eq!(elephc_parallel_job_mark_running(id), 1);
        assert!(fail_with_emergency_envelope(id));
        assert_eq!(elephc_parallel_job_wait(id), PARALLEL_JOB_FAILED);
        assert_eq!(
            elephc_parallel_job_failure_kind(id),
            PARALLEL_FAILURE_ARENA_EXHAUSTED
        );
        let mut copy = ParallelTransferBlob {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_job_output_copy(id, 2, &mut copy) },
            PARALLEL_TRANSFER_OK
        );
        let mut view = ParallelFailureView {
            kind: 0,
            remote_class: ParallelBytes {
                ptr: ptr::null(),
                len: 0,
            },
            message: ParallelBytes {
                ptr: ptr::null(),
                len: 0,
            },
            code: 0,
            file: ParallelBytes {
                ptr: ptr::null(),
                len: 0,
            },
            line: 0,
            frame_count: 0,
        };
        assert_eq!(
            unsafe { elephc_parallel_failure_view(copy, &mut view) },
            PARALLEL_TRANSFER_OK
        );
        assert_eq!(view.kind, PARALLEL_FAILURE_ARENA_EXHAUSTED);
        unsafe { crate::elephc_parallel_transfer_free(copy) };
        assert_eq!(elephc_parallel_job_release(id), 1);
    }

    /// A per-job condition-variable waiter wakes after completion from another thread.
    #[test]
    fn wait_unblocks_after_another_thread_completes() {
        let id = elephc_parallel_job_create(blob(b"input"));
        assert_eq!(elephc_parallel_job_mark_running(id), 1);
        let waiter = std::thread::spawn(move || elephc_parallel_job_wait(id));
        assert_eq!(elephc_parallel_job_complete(id, blob(b"result")), 1);
        assert_eq!(waiter.join().expect("join waiter"), PARALLEL_JOB_COMPLETED);
        assert_eq!(elephc_parallel_job_release(id), 1);
    }

    /// Releasing a terminal record never makes its id available to another job.
    #[test]
    fn job_ids_are_monotonic_and_never_reused_after_release() {
        let first = elephc_parallel_job_create(blob(b"first"));
        assert_eq!(elephc_parallel_job_cancel(first), 1);
        assert_eq!(elephc_parallel_job_release(first), 1);
        let second = elephc_parallel_job_create(blob(b"second"));
        assert!(second > first);
        assert_eq!(elephc_parallel_job_cancel(second), 1);
        assert_eq!(elephc_parallel_job_release(second), 1);
    }

    /// Generation-based group waiting observes completion without losing a racing terminal transition.
    #[test]
    fn completion_generation_wakes_group_waiters_without_lost_transitions() {
        let id = elephc_parallel_job_create(blob(b"input"));
        assert_ne!(id, 0);
        assert_eq!(elephc_parallel_job_mark_running(id), 1);
        let observed = elephc_parallel_completion_generation();
        let worker = std::thread::spawn(move || {
            assert_eq!(elephc_parallel_job_complete(id, blob(b"done")), 1);
        });
        let completed = elephc_parallel_completion_wait(observed);
        worker.join().unwrap();
        assert_ne!(completed, observed);
        assert_eq!(elephc_parallel_job_release(id), 1);
    }
}
