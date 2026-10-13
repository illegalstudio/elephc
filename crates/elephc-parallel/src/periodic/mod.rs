//! Purpose:
//! Owns native periodic registration state and wakeable scalar control commands.
//!
//! Called from:
//! - The upcoming compiler-owned TaskGroup::every pump and worker-local control proxies.
//!
//! Key details:
//! - No PHP pointer crosses threads; each registration owns a separate nonblocking wake.
//! - Handles are monotonic positive signed integers and cannot resurrect retired registrations.
//! - Release fails while an invocation is active; the pump must settle it before retirement.
//! - No background timer/thread is created: the existing coordinator polls deadlines and wakes.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::completion_wake::Wake;
use cadence::Cadence;

mod cadence;
#[cfg(test)]
mod tests;

struct Registration { cadence: Cadence, wake: Wake }

#[derive(Default)]
struct Registry { next_id: i64, entries: BTreeMap<i64, Registration> }

impl Registry {
    /// Creates one delayed registration; negative results distinguish invalid interval and transport.
    fn create(&mut self, seconds: f64, now: Instant) -> i64 {
        let Some(interval) = cadence::interval(seconds) else { return -2; };
        let Some(cadence) = Cadence::new(interval, now) else { return -2; };
        let Some(id) = self.next_id.checked_add(1) else { return -1; };
        let Ok(wake) = Wake::new() else { return -1; };
        self.next_id = id;
        self.entries.insert(id, Registration { cadence, wake });
        id
    }

    /// Retires both endpoints only when there is no active invocation.
    fn release(&mut self, id: i64) -> i64 {
        let Some(entry) = self.entries.get_mut(&id) else { return -1; };
        entry.cadence.stop();
        entry.wake.signal();
        if entry.cadence.running() { return 0; }
        self.entries.remove(&id);
        1
    }
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

/// Contains Rust unwinding and serializes command/admission order across parent and worker threads.
fn with_registry<T>(fallback: T, action: impl FnOnce(&mut Registry) -> T) -> T {
    catch_unwind(AssertUnwindSafe(|| {
        let mut registry = REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
            .lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        action(&mut registry)
    })).unwrap_or(fallback)
}

/// Allocates a native periodic identity; no timer or worker starts here.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_create(seconds: f64) -> i64 {
    with_registry(-1, |registry| registry.create(seconds, Instant::now()))
}

/// Borrows this registration's command-wake descriptor while the identity is live.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_fd(id: i64) -> i64 {
    with_registry(-1, |registry| registry.entries.get(&id).map(|entry| entry.wake.fd()).unwrap_or(-1))
}

/// Acknowledges coalesced commands; the coordinator must then re-read cadence state.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_drain(id: i64) -> i64 {
    with_registry(-1, |registry| registry.entries.get_mut(&id)
        .map(|entry| i64::from(entry.wake.drain())).unwrap_or(-1))
}

/// Returns a relative monotonic timeout; minus one denotes stopped, busy or retired state.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_remaining(id: i64) -> f64 {
    with_registry(-1.0, |registry| registry.entries.get(&id)
        .map(|entry| entry.cadence.remaining(Instant::now())).unwrap_or(-1.0))
}

/// Atomically claims a due invocation: waiting zero, admitted one, stopped/busy two, retired minus one.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_claim(id: i64) -> i64 {
    with_registry(-1, |registry| registry.entries.get_mut(&id)
        .map(|entry| entry.cadence.claim(Instant::now())).unwrap_or(-1))
}

/// Completes the active invocation before rearming or retiring its registration.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_complete(id: i64) -> i64 {
    with_registry(-1, |registry| registry.entries.get_mut(&id)
        .map(|entry| if entry.cadence.complete(Instant::now()) { 1 } else { -1 }).unwrap_or(-1))
}

/// Stops future admission and wakes a sleeping coordinator without forcibly terminating active work.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_stop(id: i64) -> i64 {
    with_registry(-1, |registry| {
        let Some(entry) = registry.entries.get_mut(&id) else { return -1; };
        entry.cadence.stop();
        entry.wake.signal();
        1
    })
}

/// Changes the addressed interval and wakes its coordinator; stop always takes precedence.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_set_interval(id: i64, seconds: f64) -> i64 {
    with_registry(-1, |registry| {
        let Some(entry) = registry.entries.get_mut(&id) else { return -1; };
        let Some(interval) = cadence::interval(seconds) else { return -2; };
        if !entry.cadence.set_interval(interval, Instant::now()) { return -1; }
        entry.wake.signal();
        1
    })
}

/// Retires a settled registration; zero requires a later retry after active work has drained.
#[no_mangle]
pub extern "C" fn elephc_task_periodic_release(id: i64) -> i64 {
    with_registry(-1, |registry| registry.release(id))
}
