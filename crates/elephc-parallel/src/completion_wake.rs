//! Purpose:
//! Supplies scope-owned pollable completion wakes for cooperative worker waiting.
//!
//! Called from:
//! - `crate::job::notify_completion()` and the unified task coordinator's C ABI.
//!
//! Key details:
//! - Each subscriber owns a separate nonblocking socket pair; wakes are broadcast.
//! - Native handles are never reused and contain no PHP pointers or arena leases.
//! - One pending byte coalesces transitions; an inactive bridge creates no sockets.
//! - Create a subscription before checking job state, then recheck after every wake.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};

pub(crate) struct Wake {
    reader: UnixStream,
    writer: UnixStream,
    pending: bool,
    failed: bool,
}

impl Wake {
    /// Creates both owned endpoints without allowing either notification or drain to block.
    pub(crate) fn new() -> io::Result<Self> {
        let (reader, writer) = UnixStream::pair()?;
        reader.set_nonblocking(true)?;
        writer.set_nonblocking(true)?;
        Ok(Self { reader, writer, pending: false, failed: false })
    }

    /// Coalesces a notification; transport failure makes the reader ready with EOF.
    pub(crate) fn signal(&mut self) {
        if self.pending || self.failed {
            return;
        }
        match (&self.writer).write_all(&[1]) {
            Ok(()) => self.pending = true,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => self.pending = true,
            Err(_) => {
                self.failed = true;
                let _ = self.writer.shutdown(Shutdown::Write);
            }
        }
    }

    /// Consumes the single wake byte under the registry lock, preserving future notifications.
    pub(crate) fn drain(&mut self) -> i32 {
        if self.failed {
            return -1;
        }
        let mut byte = [0];
        loop {
            match (&self.reader).read(&mut byte) {
                Ok(1) => { self.pending = false; return 1; }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.pending = false;
                    return 0;
                }
                _ => { self.failed = true; return -1; }
            }
        }
    }

    /// Borrows the read endpoint while this native wake owner remains live.
    pub(crate) fn fd(&self) -> i64 {
        if self.failed { -1 } else { i64::from(self.reader.as_raw_fd()) }
    }
}

#[derive(Default)]
struct Registry {
    next_id: u64,
    wakes: BTreeMap<u64, Wake>,
}

impl Registry {
    /// Registers one owned wake and reserves a monotonic nonzero handle on success.
    fn create(&mut self) -> u64 {
        let Some(id) = self.next_id.checked_add(1).filter(|id| *id <= i64::MAX as u64) else {
            return 0;
        };
        let Ok(wake) = Wake::new() else { return 0; };
        self.next_id = id;
        self.wakes.insert(id, wake);
        id
    }

    /// Borrows a descriptor only while its native registration remains live.
    fn fd(&self, id: u64) -> i32 {
        self.wakes.get(&id).filter(|wake| !wake.failed)
            .map(|wake| wake.reader.as_raw_fd()).unwrap_or(-1)
    }

    /// Drains only the addressed subscriber, never another coordinator's wake.
    fn drain(&mut self, id: u64) -> i32 {
        self.wakes.get_mut(&id).map(Wake::drain).unwrap_or(-1)
    }

    /// Removes a subscriber and closes both endpoints exactly once.
    fn release(&mut self, id: u64) -> i32 {
        if self.wakes.remove(&id).is_some() { 1 } else { -1 }
    }

    /// Broadcasts a coalesced notification without allocating per transition.
    fn signal_all(&mut self) {
        for wake in self.wakes.values_mut() {
            wake.signal();
        }
    }
}

static WAKES: OnceLock<Mutex<Registry>> = OnceLock::new();

/// Runs a native registry operation without allowing a Rust panic across the C ABI.
fn with_registry<T>(fallback: T, action: impl FnOnce(&mut Registry) -> T) -> T {
    catch_unwind(AssertUnwindSafe(|| {
        let mut registry = WAKES.get_or_init(|| Mutex::new(Registry::default()))
            .lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        action(&mut registry)
    })).unwrap_or(fallback)
}

/// Notifies live subscribers, without initializing any registry on dormant paths.
pub(crate) fn notify_waiters() {
    let Some(registry) = WAKES.get() else { return; };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        registry.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).signal_all();
    }));
}

/// Creates a pollable subscription; zero means initialization or allocation failed.
#[no_mangle]
pub extern "C" fn elephc_parallel_completion_wake_create() -> u64 {
    with_registry(0, Registry::create)
}

/// Returns a borrowed descriptor, or minus one for a retired/failed subscription.
#[no_mangle]
pub extern "C" fn elephc_parallel_completion_wake_fd(id: u64) -> i64 {
    with_registry(-1, |registry| i64::from(registry.fd(id)))
}

/// Acknowledges coalesced notifications; callers must recheck their own job states.
#[no_mangle]
pub extern "C" fn elephc_parallel_completion_wake_drain(id: u64) -> i64 {
    with_registry(-1, |registry| i64::from(registry.drain(id)))
}

/// Retires a subscription only after the owning coordinator has settled its waits.
#[no_mangle]
pub extern "C" fn elephc_parallel_completion_wake_release(id: u64) -> i64 {
    with_registry(-1, |registry| i64::from(registry.release(id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Notifications coalesce to one byte and acknowledgement rearms the same subscription.
    #[test]
    fn completion_wake_coalesces_and_rearms() {
        let mut registry = Registry::default();
        let id = registry.create();
        assert_ne!(id, 0);
        assert_eq!(registry.drain(id), 0);
        for _ in 0..10_000 { registry.signal_all(); }
        assert_eq!(registry.drain(id), 1);
        assert_eq!(registry.drain(id), 0);
        registry.signal_all();
        assert_eq!(registry.drain(id), 1);
    }

    /// Acknowledging one subscriber cannot steal a different scope's notification.
    #[test]
    fn completion_wake_broadcast_has_independent_subscribers() {
        let mut registry = Registry::default();
        let left = registry.create();
        let right = registry.create();
        assert_ne!(left, 0);
        assert_ne!(right, 0);
        assert_ne!(registry.fd(left), registry.fd(right));
        registry.signal_all();
        assert_eq!(registry.drain(left), 1);
        assert_eq!(registry.drain(left), 0);
        assert_eq!(registry.drain(right), 1);
    }

    /// Retired handles fail closed and never designate a later subscription after FD reuse.
    #[test]
    fn completion_wake_retirement_rejects_stale_handles() {
        let mut registry = Registry::default();
        let retired = registry.create();
        assert_eq!(registry.release(retired), 1);
        let live = registry.create();
        assert!(live > retired);
        assert_eq!(registry.fd(retired), -1);
        assert_eq!(registry.drain(retired), -1);
        assert_eq!(registry.release(retired), -1);
        assert!(registry.fd(live) >= 0);
    }

    /// Exhausted handle space cannot wrap or overwrite another subscription.
    #[test]
    fn completion_wake_handle_exhaustion_fails_closed() {
        let mut registry = Registry { next_id: i64::MAX as u64, wakes: BTreeMap::new() };
        assert_eq!(registry.create(), 0);
        assert!(registry.wakes.is_empty());
    }

    /// PHP extern integer results retain negative sentinel values in the full 64-bit ABI slot.
    #[test]
    fn completion_wake_c_abi_errors_are_signed_64_bit() {
        let fd: i64 = elephc_parallel_completion_wake_fd(0);
        let drain: i64 = elephc_parallel_completion_wake_drain(0);
        let release: i64 = elephc_parallel_completion_wake_release(0);
        assert_eq!((fd, drain, release), (-1, -1, -1));
    }

    /// An OS worker thread can publish readiness without touching a PHP arena.
    #[test]
    fn completion_wake_cross_thread_notification() {
        let registry = std::sync::Arc::new(Mutex::new(Registry::default()));
        let id = registry.lock().unwrap().create();
        let publisher = std::sync::Arc::clone(&registry);
        std::thread::spawn(move || publisher.lock().unwrap().signal_all()).join().unwrap();
        assert_eq!(registry.lock().unwrap().drain(id), 1);
    }
}
