//! Purpose:
//! Joins finishing workers off the cooperative coordinator's thread.
//!
//! Called from:
//! - `super::spawn_worker()` and each admitted worker's exit notice.
//!
//! Key details:
//! - Only exiting workers enter the queue, not arbitrary running jobs.
//! - Worker registry entries own the reaper; it is stopped and joined on final release.
//! - The reaper owns native IDs and thread handles only, never PHP pointers.

use std::sync::{mpsc, Arc, Mutex, OnceLock, Weak};
use std::thread::{self, JoinHandle};

pub(super) struct Reaper {
    sender: Option<mpsc::Sender<u64>>,
    thread: Option<JoinHandle<()>>,
}

pub(super) struct ExitNotice {
    id: u64,
    sender: mpsc::Sender<u64>,
}

impl Drop for ExitNotice {
    /// Schedules joining even when worker execution or admission bookkeeping unwinds.
    fn drop(&mut self) {
        let _ = self.sender.send(self.id);
    }
}

impl Reaper {
    /// Creates a notice without retaining a reaper owner on the worker or reaper thread.
    pub(super) fn notice(&self, id: u64) -> ExitNotice {
        ExitNotice { id, sender: self.sender.as_ref().expect("live reaper sender").clone() }
    }
}

impl Drop for Reaper {
    /// Closes the queue and joins its consumer before the last native scope can release code.
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

static REAPER: OnceLock<Mutex<Weak<Reaper>>> = OnceLock::new();

/// Shares one reaper while registry owners exist; initialization failure rejects worker admission.
pub(super) fn acquire() -> Option<Arc<Reaper>> {
    let mut weak = REAPER.get_or_init(|| Mutex::new(Weak::new())).lock().ok()?;
    if let Some(reaper) = weak.upgrade() {
        return Some(reaper);
    }
    let (sender, receiver) = mpsc::channel();
    let thread = thread::Builder::new().name("elephc-parallel-reaper".into())
        .spawn(move || {
            while let Ok(id) = receiver.recv() {
                super::join_worker(id);
                crate::job::notify_completion();
            }
        }).ok()?;
    let reaper = Arc::new(Reaper { sender: Some(sender), thread: Some(thread) });
    *weak = Arc::downgrade(&reaper);
    Some(reaper)
}
