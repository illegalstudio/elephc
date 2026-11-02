//! Purpose:
//! Tracks started processes and answers `proc_get_status()` / `proc_close()` / `proc_terminate()`
//! with php-src's wait semantics.
//!
//! Called from:
//! - `crate::abi` and the Magician `proc_*` adapters, keyed by an opaque token the caller owns.
//!
//! Key details:
//! - The first poll that reaps a normally exited child caches its wait status, and every later
//!   poll or close reuses it (php 8.3+). A child that died from a signal is not cached: the next
//!   poll finds the child already reaped (`ECHILD`) and reports it as no longer running.
//! - `close` returns the exit code of a normal exit, the raw wait status of any other death, and
//!   `-1` when the child was already reaped, matching `proc_close()`.

use libc::{c_int, pid_t};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// One `proc_get_status()` snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// True while the child has neither exited nor been killed.
    pub running: bool,
    /// True when a signal ended the child.
    pub signaled: bool,
    /// True when the child is stopped.
    pub stopped: bool,
    /// Exit code of a normal exit, `-1` otherwise.
    pub exitcode: i64,
    /// Signal that ended the child, 0 otherwise.
    pub termsig: i64,
    /// Signal that stopped the child, 0 otherwise.
    pub stopsig: i64,
    /// True when the wait status was cached by an earlier poll.
    pub cached: bool,
}

/// Identifies the pipe a descriptor number referred to when the process was registered.
///
/// The caller's stream resources own the descriptor numbers, so by `proc_close()` time a number
/// may have been closed and reused. Only a descriptor that is still the same pipe is touched.
struct PipeIdentity {
    fd: c_int,
    device: u64,
    inode: u64,
}

impl PipeIdentity {
    /// Records the identity of `fd`, or nothing when it is not an open pipe.
    fn of(fd: c_int) -> Option<Self> {
        let mut info = unsafe { std::mem::zeroed::<libc::stat>() };
        if unsafe { libc::fstat(fd, &mut info) } != 0 || (info.st_mode & libc::S_IFMT) != libc::S_IFIFO {
            return None;
        }
        Some(Self { fd, device: info.st_dev as u64, inode: info.st_ino as u64 })
    }

    /// Closes this pipe end without releasing its number, when the number is still this pipe.
    ///
    /// The descriptor is replaced by `/dev/null`, so the child sees end-of-file now and the
    /// caller's stream resource later closes a harmless descriptor instead of a stranger's.
    fn neutralize(&self) {
        let Some(current) = Self::of(self.fd) else { return };
        if current.device != self.device || current.inode != self.inode {
            return;
        }
        unsafe {
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
            if null >= 0 {
                libc::dup2(null, self.fd);
                libc::close(null);
            }
        }
    }
}

/// A registered process.
struct Entry {
    pipes: Vec<PipeIdentity>,
    pid: pid_t,
    command: String,
    cached_status: Option<c_int>,
    last: Snapshot,
}

/// Returns the process-wide table.
fn table() -> &'static Mutex<HashMap<i64, Entry>> {
    static TABLE: OnceLock<Mutex<HashMap<i64, Entry>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers a started process under `token`, replacing any stale entry.
///
/// A replaced entry belongs to a handle that was dropped without `proc_close()`; its child is
/// reaped without blocking so the zombie does not outlive the handle number's reuse.
pub fn register(token: i64, pid: pid_t, command: &str, pipes: &[c_int]) {
    let Ok(mut guard) = table().lock() else { return };
    if let Some(stale) = guard.insert(
        token,
        Entry {
            pipes: pipes.iter().filter_map(|fd| PipeIdentity::of(*fd)).collect(),
            pid,
            command: command.to_string(),
            cached_status: None,
            last: Snapshot { running: true, exitcode: -1, ..Snapshot::default() },
        },
    ) {
        if stale.cached_status.is_none() {
            let mut status = 0 as c_int;
            unsafe {
                libc::waitpid(stale.pid, &mut status, libc::WNOHANG);
            }
        }
    }
}

/// Returns the pid and command of a registered process.
pub fn describe(token: i64) -> Option<(pid_t, String)> {
    let guard = table().lock().ok()?;
    guard.get(&token).map(|entry| (entry.pid, entry.command.clone()))
}

/// Waits for `pid`, retrying when a signal interrupts the call.
fn wait_status(pid: pid_t, flags: c_int) -> (pid_t, c_int) {
    let mut status = 0 as c_int;
    loop {
        let reaped = unsafe { libc::waitpid(pid, &mut status, flags) };
        if reaped < 0
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
        {
            continue;
        }
        return (reaped, status);
    }
}

/// Reaps the child without blocking and returns the refreshed snapshot.
pub fn poll(token: i64) -> Option<Snapshot> {
    let mut guard = table().lock().ok()?;
    let entry = guard.get_mut(&token)?;
    let (reaped, status) = match entry.cached_status {
        Some(status) => (entry.pid, status),
        None => wait_status(entry.pid, libc::WNOHANG | libc::WUNTRACED),
    };
    let mut snapshot = Snapshot { running: true, exitcode: -1, ..Snapshot::default() };
    if reaped == entry.pid {
        if libc::WIFEXITED(status) {
            snapshot.running = false;
            snapshot.exitcode = i64::from(libc::WEXITSTATUS(status));
            entry.cached_status = Some(status);
        }
        if libc::WIFSIGNALED(status) {
            snapshot.running = false;
            snapshot.signaled = true;
            snapshot.termsig = i64::from(libc::WTERMSIG(status));
        }
        if libc::WIFSTOPPED(status) {
            snapshot.stopped = true;
            snapshot.stopsig = i64::from(libc::WSTOPSIG(status));
        }
    } else if reaped < 0 {
        snapshot.running = false;
    }
    snapshot.cached = entry.cached_status.is_some();
    entry.last = snapshot;
    Some(snapshot)
}

/// Returns the snapshot stored by the most recent `poll()`.
pub fn last_snapshot(token: i64) -> Option<Snapshot> {
    let guard = table().lock().ok()?;
    guard.get(&token).map(|entry| entry.last)
}

/// Waits for the child, forgets the entry, and returns `proc_close()`'s result.
pub fn close(token: i64) -> Option<i64> {
    let entry = {
        let mut guard = table().lock().ok()?;
        guard.remove(&token)?
    };
    for pipe in &entry.pipes {
        pipe.neutralize();
    }
    let (reaped, status) = match entry.cached_status {
        Some(status) => (entry.pid, status),
        None => wait_status(entry.pid, 0),
    };
    if reaped <= 0 {
        return Some(-1);
    }
    Some(if libc::WIFEXITED(status) {
        i64::from(libc::WEXITSTATUS(status))
    } else {
        i64::from(status)
    })
}

/// Sends `signal` to the child; true when the kernel accepted it.
pub fn terminate(token: i64, signal: c_int) -> Option<bool> {
    let guard = table().lock().ok()?;
    let entry = guard.get(&token)?;
    Some(unsafe { libc::kill(entry.pid, signal) } == 0)
}
