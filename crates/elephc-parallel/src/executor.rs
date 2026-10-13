//! Purpose:
//! Owns bounded FIFO admission and one nonpersistent OS thread per admitted Parallel job.
//!
//! Called from:
//! - The future Parallel TaskGroup submission and Future join bridge operations.
//!
//! Key details:
//! - Queued jobs own no thread until one of the seven worker context slots is available.
//! - Every admitted thread uses the configured stack size and is joined before job release.
//! - Completion admits queued jobs in submission order; no persistent worker pool exists in v1.

use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Barrier, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use elephc_parallel_contract::{
    PARALLEL_WORKER_ARENA_EXHAUSTED, PARALLEL_WORKER_CONTEXT_CAPACITY,
    PARALLEL_WORKER_TRANSFER_DECODE, PARALLEL_WORKER_TRANSFER_ENCODE, WORKER_STACK_MAX_BYTES,
    WORKER_STACK_MIN_BYTES,
};

use crate::failure::{
    simple_failure_blob, PARALLEL_FAILURE_ARENA_EXHAUSTED,
    PARALLEL_FAILURE_CONTEXT_UNAVAILABLE, PARALLEL_FAILURE_INFRASTRUCTURE,
    PARALLEL_FAILURE_TRANSFER_DECODE, PARALLEL_FAILURE_TRANSFER_ENCODE,
    PARALLEL_FAILURE_WORKER_PANIC,
};
use crate::job::{
    cancel_running_after_worker_exit, elephc_parallel_job_cancellation_requested,
    elephc_parallel_job_fail, elephc_parallel_job_mark_running, elephc_parallel_job_phase,
    fail_with_emergency_envelope, reserve_submission, unreserve_submission, PARALLEL_JOB_QUEUED,
    PARALLEL_JOB_RUNNING,
};
use crate::{
    elephc_parallel_run_worker_v1, release_transfer_blob, PARALLEL_WORKER_ALLOCATION_FAILED,
    PARALLEL_WORKER_CLEANUP_FAILED, PARALLEL_WORKER_CONTEXT_UNAVAILABLE,
    PARALLEL_WORKER_INVALID_ARGUMENT, PARALLEL_WORKER_PANIC, PARALLEL_WORKER_PHP_FATAL,
};

pub const PARALLEL_EXECUTOR_ERROR: i32 = -1;
pub const PARALLEL_EXECUTOR_REJECTED: i32 = 0;
pub const PARALLEL_EXECUTOR_ACCEPTED: i32 = 1;

#[derive(Clone, Copy)]
struct WorkerSpec {
    id: u64,
    acquire_addr: usize,
    release_addr: usize,
    entry_addr: usize,
    callback_addr: usize,
    heap_size: usize,
    stack_size: usize,
}

#[derive(Default)]
struct ExecutorState {
    active: usize,
    queued: VecDeque<WorkerSpec>,
}

enum JoinState {
    Pending,
    Running(JoinHandle<()>),
    Joining,
    Joined(bool),
}

struct WorkerJoin {
    state: Mutex<JoinState>,
    changed: Condvar,
}

static EXECUTOR: OnceLock<Mutex<ExecutorState>> = OnceLock::new();
static WORKERS: OnceLock<Mutex<HashMap<u64, Arc<WorkerJoin>>>> = OnceLock::new();

/// Initializes the bounded admission counter and FIFO queue under one executor mutex.
fn executor() -> &'static Mutex<ExecutorState> {
    EXECUTOR.get_or_init(|| Mutex::new(ExecutorState::default()))
}

/// Initializes the join-record registry kept until terminal job release forgets each worker.
fn workers() -> &'static Mutex<HashMap<u64, Arc<WorkerJoin>>> {
    WORKERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Checks nonzero job/runtime addresses, heap capacity and reviewed stack-size bounds before submission.
fn valid_spec(spec: WorkerSpec) -> bool {
    spec.id != 0
        && spec.acquire_addr != 0
        && spec.release_addr != 0
        && spec.entry_addr != 0
        && spec.callback_addr != 0
        && spec.heap_size != 0
        && (WORKER_STACK_MIN_BYTES..=WORKER_STACK_MAX_BYTES).contains(&spec.stack_size)
}

/// Submits a job to the v1 executor. Queued jobs remain threadless until admitted.
#[no_mangle]
pub extern "C" fn elephc_parallel_job_submit_v1(
    id: u64,
    acquire_addr: usize,
    release_addr: usize,
    entry_addr: usize,
    callback_addr: usize,
    heap_size: usize,
    stack_size: usize,
) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| {
        submit(WorkerSpec {
            id,
            acquire_addr,
            release_addr,
            entry_addr,
            callback_addr,
            heap_size,
            stack_size,
        })
    })) {
        Ok(status) => status,
        Err(_) => PARALLEL_EXECUTOR_ERROR,
    }
}

/// Reserves one submission, queues it and admits available work, rolling back on lock failure.
fn submit(spec: WorkerSpec) -> i32 {
    if !valid_spec(spec) {
        return PARALLEL_EXECUTOR_ERROR;
    }
    if !reserve_submission(spec.id) {
        return PARALLEL_EXECUTOR_REJECTED;
    }
    let Ok(mut state) = executor().lock() else {
        unreserve_submission(spec.id);
        return PARALLEL_EXECUTOR_ERROR;
    };
    state.queued.push_back(spec);
    admit_available(&mut state);
    PARALLEL_EXECUTOR_ACCEPTED
}

/// Admits queued specs in FIFO order up to capacity, skipping cancelled jobs without spawning threads.
fn admit_available(state: &mut ExecutorState) {
    while state.active < PARALLEL_WORKER_CONTEXT_CAPACITY {
        let Some(spec) = state.queued.pop_front() else {
            return;
        };
        if elephc_parallel_job_phase(spec.id) != PARALLEL_JOB_QUEUED {
            continue;
        }
        if spawn_worker(spec) {
            state.active += 1;
        } else {
            unreserve_submission(spec.id);
            if elephc_parallel_job_mark_running(spec.id) != 1 {
                continue;
            }
            publish_failure(
                spec.id,
                PARALLEL_FAILURE_INFRASTRUCTURE,
                b"Parallel worker thread creation failed",
            );
        }
    }
}

/// Installs a join record before releasing a fresh, stack-sized OS thread through its startup barrier.
fn spawn_worker(spec: WorkerSpec) -> bool {
    let gate = Arc::new(Barrier::new(2));
    let join = Arc::new(WorkerJoin {
        state: Mutex::new(JoinState::Pending),
        changed: Condvar::new(),
    });
    {
        let Ok(mut workers) = workers().lock() else {
            return false;
        };
        workers.insert(spec.id, Arc::clone(&join));
    }

    let worker_gate = Arc::clone(&gate);
    let thread = thread::Builder::new()
        .name(format!("elephc-parallel-{}", spec.id))
        .stack_size(spec.stack_size)
        .spawn(move || {
            worker_gate.wait();
            let result = catch_unwind(AssertUnwindSafe(|| execute(spec)));
            if result.is_err() {
                publish_failure(
                    spec.id,
                    PARALLEL_FAILURE_WORKER_PANIC,
                    b"Parallel worker thread panicked",
                );
            }
            worker_finished();
        });
    let Ok(thread) = thread else {
        if let Ok(mut workers) = workers().lock() {
            workers.remove(&spec.id);
        }
        return false;
    };
    let mut state = join
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *state = JoinState::Running(thread);
    drop(state);
    gate.wait();
    true
}

/// Marks one admitted job Running and invokes the isolated runtime bridge before enforcing settlement.
fn execute(spec: WorkerSpec) {
    if elephc_parallel_job_mark_running(spec.id) != 1 {
        return;
    }
    let status = unsafe {
        elephc_parallel_run_worker_v1(
            spec.acquire_addr,
            spec.release_addr,
            spec.entry_addr,
            spec.callback_addr,
            spec.id as usize as *mut c_void,
            spec.heap_size,
            spec.stack_size,
        )
    };
    ensure_terminal(spec.id, status);
}

/// Settles an otherwise-running exit as cancellation or the typed bridge/runtime failure category.
fn ensure_terminal(id: u64, status: i64) {
    if elephc_parallel_job_phase(id) != PARALLEL_JOB_RUNNING {
        return;
    }
    if elephc_parallel_job_cancellation_requested(id) == 1
        && cancel_running_after_worker_exit(id)
    {
        return;
    }
    let (kind, message): (i32, &[u8]) = match status {
        PARALLEL_WORKER_CONTEXT_UNAVAILABLE => (
            PARALLEL_FAILURE_CONTEXT_UNAVAILABLE,
            b"Parallel worker context was unavailable",
        ),
        PARALLEL_WORKER_ALLOCATION_FAILED => (
            PARALLEL_FAILURE_ARENA_EXHAUSTED,
            b"Parallel worker arena allocation failed",
        ),
        PARALLEL_WORKER_PANIC => (
            PARALLEL_FAILURE_WORKER_PANIC,
            b"Parallel worker bridge panicked",
        ),
        PARALLEL_WORKER_CLEANUP_FAILED => (
            PARALLEL_FAILURE_INFRASTRUCTURE,
            b"Parallel worker context cleanup failed",
        ),
        PARALLEL_WORKER_PHP_FATAL => (
            crate::PARALLEL_FAILURE_PHP_FATAL,
            b"Parallel worker terminated with a PHP fatal",
        ),
        PARALLEL_WORKER_ARENA_EXHAUSTED => (
            PARALLEL_FAILURE_ARENA_EXHAUSTED,
            b"Parallel worker transfer allocation failed",
        ),
        PARALLEL_WORKER_TRANSFER_DECODE => (
            PARALLEL_FAILURE_TRANSFER_DECODE,
            b"Parallel worker could not decode its input payload",
        ),
        PARALLEL_WORKER_TRANSFER_ENCODE => (
            PARALLEL_FAILURE_TRANSFER_ENCODE,
            b"Parallel worker could not encode its terminal payload",
        ),
        PARALLEL_WORKER_INVALID_ARGUMENT => (
            PARALLEL_FAILURE_INFRASTRUCTURE,
            b"Parallel worker received an invalid runtime contract",
        ),
        _ => (
            PARALLEL_FAILURE_INFRASTRUCTURE,
            b"Parallel worker returned without completing its job",
        ),
    };
    publish_failure(id, kind, message);
}

/// Transfers an owned failure envelope to a running job, using its emergency envelope on allocation failure.
fn publish_failure(id: u64, kind: i32, message: &[u8]) {
    if elephc_parallel_job_phase(id) != PARALLEL_JOB_RUNNING {
        return;
    }
    let Ok(blob) = simple_failure_blob(kind, message) else {
        fail_with_emergency_envelope(id);
        return;
    };
    if elephc_parallel_job_fail(id, blob) != 1 {
        unsafe { release_transfer_blob(blob) };
    }
}

/// Returns one admission slot after worker execution and admits the next queued specs.
fn worker_finished() {
    let Ok(mut state) = executor().lock() else {
        return;
    };
    state.active = state.active.saturating_sub(1);
    admit_available(&mut state);
}

/// Joins a worker once and shares that result with racing waiters without holding the join lock during join.
pub(crate) fn join_worker(id: u64) -> bool {
    let join = {
        let Ok(workers) = workers().lock() else {
            return false;
        };
        workers.get(&id).cloned()
    };
    let Some(join) = join else {
        return true;
    };
    let mut state = match join.state.lock() {
        Ok(state) => state,
        Err(_) => return false,
    };
    loop {
        match &*state {
            JoinState::Pending | JoinState::Joining => {
                let Ok(next) = join.changed.wait(state) else {
                    return false;
                };
                state = next;
            }
            JoinState::Joined(ok) => return *ok,
            JoinState::Running(_) => {
                let JoinState::Running(handle) =
                    std::mem::replace(&mut *state, JoinState::Joining)
                else {
                    unreachable!();
                };
                drop(state);
                let ok = handle.join().is_ok();
                let Ok(mut state) = join.state.lock() else {
                    return false;
                };
                *state = JoinState::Joined(ok);
                join.changed.notify_all();
                return ok;
            }
        }
    }
}

/// Removes join bookkeeping after the owning terminal job has completed its join/release protocol.
pub(crate) fn forget_worker(id: u64) {
    if let Ok(mut workers) = workers().lock() {
        workers.remove(&id);
    }
}

#[cfg(test)]
mod tests;
