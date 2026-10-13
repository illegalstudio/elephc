//! Purpose:
//! Concurrency tests for bounded FIFO Parallel worker admission and joining.
//!
//! Called from:
//! - `cargo test -p elephc-parallel` through `crate::executor`.
//!
//! Key details:
//! - Holds admitted callbacks behind a gate to prove queued tasks own no thread.
//! - Releases one slot first to prove the next submitted job is admitted before later work.

use std::collections::HashSet;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use elephc_parallel_contract::{
    PARALLEL_WORKER_ARENA_EXHAUSTED, PARALLEL_WORKER_CONTEXT_CAPACITY,
    PARALLEL_WORKER_TRANSFER_DECODE, PARALLEL_WORKER_TRANSFER_ENCODE,
    WORKER_STACK_DEFAULT_BYTES,
};

use super::*;
use crate::job::{
    elephc_parallel_job_cancel, elephc_parallel_job_complete, elephc_parallel_job_create,
    elephc_parallel_job_failure_kind, elephc_parallel_job_phase, elephc_parallel_job_release,
    elephc_parallel_job_wait, PARALLEL_JOB_CANCELLED, PARALLEL_JOB_COMPLETED, PARALLEL_JOB_FAILED,
    PARALLEL_JOB_QUEUED, PARALLEL_JOB_RUNNING,
};
use crate::{
    elephc_parallel_job_complete_php_serialized, elephc_parallel_job_fail_php_serialized,
    elephc_parallel_job_failure_php_prepare, elephc_parallel_job_result_php_serialized,
    elephc_parallel_php_blob_len, elephc_parallel_php_blob_ptr, elephc_parallel_php_blob_release,
    elephc_parallel_value_write_string, elephc_parallel_value_writer_finish,
    elephc_parallel_value_writer_new, ParallelTransferBlob, PARALLEL_FAILURE_ARENA_EXHAUSTED,
    PARALLEL_FAILURE_INFRASTRUCTURE, PARALLEL_FAILURE_TRANSFER_DECODE,
    PARALLEL_FAILURE_TRANSFER_ENCODE, PARALLEL_TRANSFER_OK,
};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default)]
struct GateState {
    started: Vec<(u64, String)>,
    released: HashSet<u64>,
}

static GATE: OnceLock<(Mutex<GateState>, Condvar)> = OnceLock::new();
static CALLBACK_EXITED: AtomicBool = AtomicBool::new(false);
static RELEASED_CONTEXTS: AtomicUsize = AtomicUsize::new(0);

/// Initializes the test gate used to observe admission before allowing callbacks to finish.
fn gate() -> &'static (Mutex<GateState>, Condvar) {
    GATE.get_or_init(|| (Mutex::new(GateState::default()), Condvar::new()))
}

/// Encodes a test string into an owned EPV input/result blob.
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

/// Test-only acquisition returns a non-dereferenced context sentinel, not a real runtime slot.
unsafe extern "C" fn acquire_context(
    _arena: *mut u8,
    _heap_size: usize,
    _object_index: *mut u32,
    _object_free: *mut u32,
    _buffer_registry: *mut u8,
    _serialize_object_ptrs: *mut u64,
    _serialize_object_indexes: *mut u64,
    _unserialize_values: *mut u64,
) -> *mut c_void {
    1usize as *mut c_void
}

/// Counts test context releases and permits backing cleanup without operating on runtime memory.
unsafe extern "C" fn release_context(_context: *mut c_void) -> i64 {
    RELEASED_CONTEXTS.fetch_add(1, Ordering::Release);
    1
}

/// Test entry forwards the callback and leaves release acknowledgment to bridge fallback cleanup.
unsafe extern "C" fn worker_entry(
    _context: *mut c_void,
    callback: crate::WorkerCallback,
    job: *mut c_void,
    _stack_size: usize,
    _release_status: *mut i64,
) -> i64 {
    unsafe { callback(job) }
}

const _: crate::WorkerEntry = worker_entry;

/// Records its real OS-thread id, waits behind the admission gate, then settles an owned result.
unsafe extern "C" fn gated_callback(job: *mut c_void) -> i64 {
    let id = job as usize as u64;
    let thread = format!("{:?}", thread::current().id());
    let (state, changed) = gate();
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.started.push((id, thread));
    changed.notify_all();
    while !state.released.contains(&id) {
        state = changed
            .wait(state)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
    drop(state);
    assert_eq!(elephc_parallel_job_complete(id, blob(b"done")), 1);
    0
}

/// Returns success without settling a record to exercise the executor's missing-outcome failure.
unsafe extern "C" fn incomplete_callback(_job: *mut c_void) -> i64 {
    0
}

/// Injects the typed transfer-decode status without emulating a PHP decoder.
unsafe extern "C" fn transfer_decode_callback(_job: *mut c_void) -> i64 {
    PARALLEL_WORKER_TRANSFER_DECODE
}

/// Injects the typed transfer-encode status without producing a terminal value blob.
unsafe extern "C" fn transfer_encode_callback(_job: *mut c_void) -> i64 {
    PARALLEL_WORKER_TRANSFER_ENCODE
}

/// Injects the worker arena/transfer allocation failure status for category preservation.
unsafe extern "C" fn arena_exhausted_callback(_job: *mut c_void) -> i64 {
    PARALLEL_WORKER_ARENA_EXHAUSTED
}

/// Settles before sleeping briefly so waiters must join the owned thread rather than just its phase.
unsafe extern "C" fn complete_before_exit_callback(job: *mut c_void) -> i64 {
    let id = job as usize as u64;
    assert_eq!(elephc_parallel_job_complete(id, blob(b"done")), 1);
    thread::sleep(Duration::from_millis(50));
    CALLBACK_EXITED.store(true, Ordering::Release);
    0
}

/// Settles a PHP serialized string through the real terminal-payload adapter.
unsafe extern "C" fn complete_php_serialized_callback(job: *mut c_void) -> i64 {
    let id = job as usize as u64;
    let value = b"s:4:\"done\";";
    if unsafe { elephc_parallel_job_complete_php_serialized(id, value.as_ptr(), value.len()) } == 1 {
        0
    } else {
        crate::PARALLEL_WORKER_INVALID_ARGUMENT
    }
}

/// Settles a PHP failure-field array through the real failure-envelope adapter.
unsafe extern "C" fn fail_php_serialized_callback(job: *mut c_void) -> i64 {
    let id = job as usize as u64;
    let failure = b"a:7:{i:0;i:1;i:1;s:11:\"DomainError\";i:2;s:4:\"boom\";i:3;i:7;i:4;s:8:\"task.php\";i:5;i:41;i:6;a:0:{}}";
    if unsafe { elephc_parallel_job_fail_php_serialized(id, failure.as_ptr(), failure.len()) } == 1 {
        0
    } else {
        crate::PARALLEL_WORKER_INVALID_ARGUMENT
    }
}

/// Copies a test terminal payload before releasing its native transfer allocation.
fn php_payload(blob: ParallelTransferBlob) -> Vec<u8> {
    assert!(!blob.ptr.is_null());
    assert!(blob.len >= crate::TRANSFER_HEADER_LEN);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            blob.ptr.add(crate::TRANSFER_HEADER_LEN),
            blob.len - crate::TRANSFER_HEADER_LEN,
        )
    }
    .to_vec();
    unsafe { crate::elephc_parallel_transfer_free(blob) };
    bytes
}

/// Waits with a bounded deadline for admission observations and returns their ordered snapshot.
fn wait_for_started(count: usize) -> Vec<(u64, String)> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let (state, changed) = gate();
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    while state.started.len() < count {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("worker admission timed out");
        let (next, timeout) = changed
            .wait_timeout(state, remaining)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state = next;
        assert!(!timeout.timed_out(), "worker admission timed out");
    }
    state.started.clone()
}

/// Opens selected callback gates while leaving other admitted workers blocked.
fn release(ids: impl IntoIterator<Item = u64>) {
    let (state, changed) = gate();
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.released.extend(ids);
    changed.notify_all();
}

/// Submits a fixture job with the observable gated callback.
fn submit(id: u64) -> i32 {
    submit_with(id, gated_callback)
}

/// Submits a typed fixture callback using its reviewed native entry address.
fn submit_with(id: u64, callback: crate::WorkerCallback) -> i32 {
    submit_with_addr(id, callback as *const () as usize)
}

/// Uses the test context/entry callbacks and default stack size for one executor submission.
fn submit_with_addr(id: u64, callback_addr: usize) -> i32 {
    elephc_parallel_job_submit_v1(
        id,
        acquire_context as *const () as usize,
        release_context as *const () as usize,
        worker_entry as *const () as usize,
        callback_addr,
        4096,
        WORKER_STACK_DEFAULT_BYTES,
    )
}

/// FIFO admission obeys capacity, keeps queued jobs threadless and joins owned threads before release.
#[test]
fn admission_is_bounded_fifo_thread_owned_and_joined_before_release() {
    let _serial = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    {
        let (state, _) = gate();
        *state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = GateState::default();
    }
    let parent_thread = format!("{:?}", thread::current().id());
    let job_count = PARALLEL_WORKER_CONTEXT_CAPACITY + 2;
    let ids: Vec<u64> = (0..job_count)
        .map(|_| elephc_parallel_job_create(blob(b"input")))
        .collect();
    assert!(ids.iter().all(|id| *id != 0));

    for &id in &ids {
        assert_eq!(submit(id), PARALLEL_EXECUTOR_ACCEPTED);
    }
    assert_eq!(submit(ids[0]), PARALLEL_EXECUTOR_REJECTED);

    let started = wait_for_started(PARALLEL_WORKER_CONTEXT_CAPACITY);
    let started_ids: HashSet<u64> = started.iter().map(|(id, _)| *id).collect();
    assert_eq!(started_ids, ids[..PARALLEL_WORKER_CONTEXT_CAPACITY].iter().copied().collect());
    assert!(started.iter().all(|(_, thread)| thread != &parent_thread));
    assert!(ids[..PARALLEL_WORKER_CONTEXT_CAPACITY]
        .iter()
        .all(|id| elephc_parallel_job_phase(*id) == PARALLEL_JOB_RUNNING));
    assert_eq!(
        elephc_parallel_job_phase(ids[PARALLEL_WORKER_CONTEXT_CAPACITY]),
        PARALLEL_JOB_QUEUED
    );
    assert_eq!(
        elephc_parallel_job_phase(ids[PARALLEL_WORKER_CONTEXT_CAPACITY + 1]),
        PARALLEL_JOB_QUEUED
    );
    let cancelled = ids[PARALLEL_WORKER_CONTEXT_CAPACITY + 1];
    assert_eq!(elephc_parallel_job_cancel(cancelled), 1);
    assert_eq!(elephc_parallel_job_wait(cancelled), PARALLEL_JOB_CANCELLED);

    release([ids[0]]);
    let started = wait_for_started(PARALLEL_WORKER_CONTEXT_CAPACITY + 1);
    assert!(started
        .iter()
        .any(|(id, _)| *id == ids[PARALLEL_WORKER_CONTEXT_CAPACITY]));
    assert!(!started
        .iter()
        .any(|(id, _)| *id == ids[PARALLEL_WORKER_CONTEXT_CAPACITY + 1]));

    release(ids[1..=PARALLEL_WORKER_CONTEXT_CAPACITY].iter().copied());

    for &id in &ids[..=PARALLEL_WORKER_CONTEXT_CAPACITY] {
        assert_eq!(elephc_parallel_job_wait(id), PARALLEL_JOB_COMPLETED);
        assert_eq!(elephc_parallel_job_release(id), 1);
    }
    let started = wait_for_started(job_count - 1);
    assert!(!started.iter().any(|(id, _)| *id == cancelled));
    assert_eq!(elephc_parallel_job_release(cancelled), 1);
}

/// Concurrent waiters join actual callback exit, while a missing terminal payload becomes failure.
#[test]
fn wait_joins_the_owned_thread_and_missing_outcomes_fail_closed() {
    let _serial = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    CALLBACK_EXITED.store(false, Ordering::Release);

    let joined = elephc_parallel_job_create(blob(b"joined"));
    assert_eq!(
        submit_with(joined, complete_before_exit_callback),
        PARALLEL_EXECUTOR_ACCEPTED
    );
    let first_waiter = thread::spawn(move || elephc_parallel_job_wait(joined));
    let second_waiter = thread::spawn(move || elephc_parallel_job_wait(joined));
    assert_eq!(
        first_waiter.join().expect("first Future waiter"),
        PARALLEL_JOB_COMPLETED
    );
    assert_eq!(
        second_waiter.join().expect("second Future waiter"),
        PARALLEL_JOB_COMPLETED
    );
    assert!(CALLBACK_EXITED.load(Ordering::Acquire));
    assert_eq!(elephc_parallel_job_release(joined), 1);

    let incomplete = elephc_parallel_job_create(blob(b"incomplete"));
    assert_eq!(
        submit_with(incomplete, incomplete_callback),
        PARALLEL_EXECUTOR_ACCEPTED
    );
    assert_eq!(elephc_parallel_job_wait(incomplete), PARALLEL_JOB_FAILED);
    assert_eq!(
        elephc_parallel_job_failure_kind(incomplete),
        PARALLEL_FAILURE_INFRASTRUCTURE
    );
    assert_eq!(elephc_parallel_job_release(incomplete), 1);
}

/// Parent copies of PHP result/failure payloads remain valid after test worker context release.
#[test]
fn php_terminal_payloads_outlive_worker_context_release() {
    let _serial = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    RELEASED_CONTEXTS.store(0, Ordering::Release);
    let completed = elephc_parallel_job_create(blob(b"completed"));
    assert_ne!(completed, 0);
    assert_eq!(
        submit_with(completed, complete_php_serialized_callback),
        PARALLEL_EXECUTOR_ACCEPTED
    );
    assert_eq!(elephc_parallel_job_wait(completed), PARALLEL_JOB_COMPLETED);
    assert_eq!(RELEASED_CONTEXTS.load(Ordering::Acquire), 1);

    let mut result = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_job_result_php_serialized(completed, &mut result) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(php_payload(result), b"s:4:\"done\";");
    assert_eq!(elephc_parallel_job_release(completed), 1);

    RELEASED_CONTEXTS.store(0, Ordering::Release);
    let failed = elephc_parallel_job_create(blob(b"failed"));
    assert_ne!(failed, 0);
    assert_eq!(
        submit_with(failed, fail_php_serialized_callback),
        PARALLEL_EXECUTOR_ACCEPTED
    );
    assert_eq!(elephc_parallel_job_wait(failed), PARALLEL_JOB_FAILED);
    assert_eq!(RELEASED_CONTEXTS.load(Ordering::Acquire), 1);

    assert_eq!(
        elephc_parallel_job_failure_php_prepare(failed),
        PARALLEL_TRANSFER_OK
    );
    let failure = unsafe {
        std::slice::from_raw_parts(
            elephc_parallel_php_blob_ptr(),
            elephc_parallel_php_blob_len(),
        )
    }
    .to_vec();
    assert_eq!(failure, b"a:7:{i:0;i:1;i:1;s:11:\"DomainError\";i:2;s:4:\"boom\";i:3;i:7;i:4;s:8:\"task.php\";i:5;i:41;i:6;a:0:{}}");
    elephc_parallel_php_blob_release();
    assert_eq!(elephc_parallel_job_release(failed), 1);
}

/// Decode, encode and arena statuses preserve distinct machine-visible failure categories.
#[test]
fn worker_transfer_statuses_preserve_machine_visible_failure_kinds() {
    let _serial = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cases: [(crate::WorkerCallback, i32); 3] = [
        (transfer_decode_callback, PARALLEL_FAILURE_TRANSFER_DECODE),
        (transfer_encode_callback, PARALLEL_FAILURE_TRANSFER_ENCODE),
        (arena_exhausted_callback, PARALLEL_FAILURE_ARENA_EXHAUSTED),
    ];
    for (callback, expected_kind) in cases {
        let id = elephc_parallel_job_create(blob(b"input"));
        assert_ne!(id, 0);
        assert_eq!(submit_with(id, callback), PARALLEL_EXECUTOR_ACCEPTED);
        assert_eq!(elephc_parallel_job_wait(id), PARALLEL_JOB_FAILED);
        assert_eq!(elephc_parallel_job_failure_kind(id), expected_kind);
        assert_eq!(elephc_parallel_job_release(id), 1);
    }
}

/// A controlled bridge panic settles, wakes, joins and releases its test context instead of stalling.
#[test]
fn controlled_bridge_panic_wakes_waiter_joins_worker_and_releases_context() {
    let _serial = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    RELEASED_CONTEXTS.store(0, Ordering::Release);
    let id = elephc_parallel_job_create(blob(b"panic"));
    assert_ne!(id, 0);
    assert_eq!(
        submit_with_addr(id, crate::TEST_PANIC_CALLBACK_ADDR),
        PARALLEL_EXECUTOR_ACCEPTED
    );
    assert_eq!(elephc_parallel_job_wait(id), PARALLEL_JOB_FAILED);
    assert_eq!(
        elephc_parallel_job_failure_kind(id),
        PARALLEL_FAILURE_WORKER_PANIC
    );
    assert_eq!(RELEASED_CONTEXTS.load(Ordering::Acquire), 1);
    assert_eq!(elephc_parallel_job_release(id), 1);
    assert!(!workers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(&id));
}
