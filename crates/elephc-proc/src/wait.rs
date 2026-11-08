//! Purpose:
//! Owns one spawned child's wait state: php-src's `waitpid_cached()`, the field decoding
//! behind `proc_get_status()`, and the blocking reap behind `proc_close()`.
//!
//! Called from:
//! - The `elephc_proc_poll` / `elephc_proc_status_field` / `elephc_proc_close` C ABI in `crate`.
//!
//! Key details:
//! - ONLY A NORMAL EXIT IS CACHED, exactly like php-src (PHP 8.3+): a stopped child may
//!   continue, and a signal death is reported once and then forgotten, so a later
//!   `proc_close()` of a child already reaped through `proc_get_status()` answers -1.
//! - Status decoding goes through libc's `WIF*` helpers so Darwin and Linux layouts agree.
//! - `proc_close()` RELEASES THE PIPES FIRST, like php-src's resource destructor, so a child
//!   blocked on stdin sees EOF instead of deadlocking the wait. The PHP streams own those
//!   descriptor numbers, so each pipe end is swapped for `/dev/null` rather than closed
//!   (a closed number could be reused and then closed by a stale `fclose()`), and only
//!   while it still names the very pipe spawned:
//!   the `(st_dev, st_ino)` identity recorded at spawn time must still match. A pipe the
//!   program already closed (EBADF) or whose number was reused (another identity) is left
//!   alone, so the bridge never closes a descriptor it no longer owns.

use std::ffi::CString;
use std::os::raw::c_int;

/// `proc_get_status()` field selectors shared with the prelude.
pub const FIELD_RUNNING: c_int = 0;
/// Whether the last poll saw a signal death.
pub const FIELD_SIGNALED: c_int = 1;
/// Whether the last poll saw a stop.
pub const FIELD_STOPPED: c_int = 2;
/// The exit code, or -1 while unknown.
pub const FIELD_EXITCODE: c_int = 3;
/// The terminating signal number, or 0.
pub const FIELD_TERMSIG: c_int = 4;
/// The stopping signal number, or 0.
pub const FIELD_STOPSIG: c_int = 5;
/// Whether the exit status is cached.
pub const FIELD_CACHED: c_int = 6;

/// One child process registered by a successful `proc_open()`.
pub(crate) struct Process {
    /// The child's process id.
    pub(crate) pid: libc::pid_t,
    /// `command` as `proc_get_status()` reports it.
    pub(crate) command: CString,
    /// `(child descriptor number, parent pipe end)` for every pipe, in spec order.
    pub(crate) pipes: Vec<(i64, c_int)>,
    /// The `(st_dev, st_ino)` identity of each pipe end, parallel to `pipes`.
    identities: Vec<Option<(u64, u64)>>,
    /// The cached normal-exit wait status, once reaped.
    cached: Option<c_int>,
    /// The fields decoded by the last [`Process::poll`].
    fields: [i64; 7],
}

impl Process {
    /// Registers a freshly spawned child with no status observed yet.
    pub(crate) fn new(pid: libc::pid_t, command: CString, pipes: Vec<(i64, c_int)>) -> Self {
        let identities = pipes.iter().map(|(_, fd)| identity(*fd)).collect();
        Self { pid, command, pipes, identities, cached: None, fields: [1, 0, 0, -1, 0, 0, 0] }
    }

    /// Releases every pipe end that still names the pipe it was spawned with.
    ///
    /// The descriptor NUMBER stays owned by the PHP stream boxed around it, which a later
    /// `fclose()` or scope exit still closes. Closing it here would free the number for the
    /// next `fopen()`, and that stale `fclose()` would then close an unrelated file. So the
    /// pipe end is replaced in place by `/dev/null` (`dup2`), which drops the pipe — the
    /// child sees EOF or `EPIPE` — while the number stays reserved. `close()` is only the
    /// fallback when `/dev/null` cannot be opened.
    fn close_pipes(&mut self) {
        let null = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        for ((_, fd), spawned) in self.pipes.drain(..).zip(self.identities.drain(..)) {
            if spawned.is_none() || identity(fd) != spawned {
                continue;
            }
            if null >= 0 && unsafe { libc::dup2(null, fd) } == fd {
                // dup2 clears close-on-exec on the target; keep the placeholder private.
                unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
            } else {
                crate::spawn::close_quietly(fd);
            }
        }
        crate::spawn::close_quietly(null);
    }

    /// php-src's `waitpid_cached()`: answers the cached status when there is one, else
    /// waits with `options` and caches a normal exit.
    fn waitpid_cached(&mut self, options: c_int) -> (libc::pid_t, c_int) {
        if let Some(status) = self.cached {
            return (self.pid, status);
        }
        let mut status: c_int = 0;
        let pid = unsafe { libc::waitpid(self.pid, &mut status, options) };
        if pid > 0 && libc::WIFEXITED(status) {
            self.cached = Some(status);
        }
        (pid, status)
    }

    /// Runs one non-blocking `proc_get_status()` observation and records its fields.
    pub(crate) fn poll(&mut self) {
        let mut running = 1;
        let (mut signaled, mut stopped) = (0, 0);
        let (mut exitcode, mut termsig, mut stopsig) = (-1, 0, 0);
        let (pid, status) = self.waitpid_cached(libc::WNOHANG | libc::WUNTRACED);
        if pid == self.pid {
            if libc::WIFEXITED(status) {
                running = 0;
                exitcode = i64::from(libc::WEXITSTATUS(status));
            }
            if libc::WIFSIGNALED(status) {
                running = 0;
                signaled = 1;
                termsig = i64::from(libc::WTERMSIG(status));
            }
            if libc::WIFSTOPPED(status) {
                stopped = 1;
                stopsig = i64::from(libc::WSTOPSIG(status));
            }
        } else if pid == -1 {
            // ECHILD: the child was already reaped (a reported signal death).
            running = 0;
        }
        let cached = i64::from(self.cached.is_some());
        self.fields = [running, signaled, stopped, exitcode, termsig, stopsig, cached];
    }

    /// Returns one field decoded by the last poll, or -1 for an unknown selector.
    pub(crate) fn field(&self, field: c_int) -> i64 {
        usize::try_from(field)
            .ok()
            .and_then(|index| self.fields.get(index).copied())
            .unwrap_or(-1)
    }

    /// `proc_close()`'s blocking reap: the exit code of a normal exit, the raw wait status
    /// otherwise, and -1 when the child can no longer be waited for.
    pub(crate) fn close(&mut self) -> i64 {
        self.close_pipes();
        loop {
            let (pid, status) = self.waitpid_cached(0);
            if pid == -1 && crate::spawn::last_errno() == libc::EINTR {
                continue;
            }
            if pid <= 0 {
                return -1;
            }
            return if libc::WIFEXITED(status) {
                i64::from(libc::WEXITSTATUS(status))
            } else {
                i64::from(status)
            };
        }
    }
}

/// Returns the `(st_dev, st_ino)` identity of an open descriptor, or `None`.
fn identity(fd: c_int) -> Option<(u64, u64)> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    Some((stat.st_dev as u64, stat.st_ino as u64))
}
