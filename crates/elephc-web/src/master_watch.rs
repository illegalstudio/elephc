//! Purpose:
//! Lets a prefork worker notice that its master is gone and recycle itself, so a
//! master killed by `SIGKILL` (or the OOM killer) does not leave workers serving
//! unsupervised on its port.
//!
//! Called from:
//! - `crate::server::spawn_worker`, which records the master PID in the child.
//! - `crate::worker::serve` and `crate::isolated_worker::serve`, which start the watch.
//!
//! Key details:
//! - The master PID is captured before `fork()`: a worker compares `getppid()` with
//!   it rather than with 1, because an orphan is reparented to a subreaper (tini,
//!   systemd) when one exists, not always to init.
//! - The watch only requests the existing `--max-requests` recycle, so the worker
//!   stops accepting, drains its in-flight connections, and exits through the same
//!   path; a broker exits on its own once its worker is gone.

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use tokio::sync::watch;

/// How often a worker checks that its master is still its parent.
const MASTER_POLL: Duration = Duration::from_millis(500);

/// PID of the master that forked this worker; 0 outside a forked worker.
static MASTER_PID: AtomicI32 = AtomicI32::new(0);

/// Records, in a freshly forked worker, the master PID read before `fork()`.
pub(crate) fn set_master_pid(master: libc::pid_t) {
    MASTER_PID.store(master, Ordering::Relaxed);
}

/// Spawns a local task that requests a graceful recycle once the master is gone.
///
/// Must run inside the worker's `LocalSet`. Does nothing when no master was
/// recorded, so a worker started some other way is never recycled by it.
pub(crate) fn spawn(recycle: watch::Sender<bool>) {
    let master = MASTER_PID.load(Ordering::Relaxed);
    if master <= 0 {
        return;
    }
    tokio::task::spawn_local(async move {
        let mut interval = tokio::time::interval(MASTER_POLL);
        loop {
            interval.tick().await;
            if unsafe { libc::getppid() } != master {
                let _ = recycle.send(true);
                return;
            }
        }
    });
}
