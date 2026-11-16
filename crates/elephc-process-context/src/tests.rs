//! Purpose:
//! Checks nested callback masks, process inheritance, and restoration on the calling thread.
//!
//! Called from:
//! - Focused elephc-process-context unit tests.
//!
//! Key details:
//! - Tests change only their own thread mask and always restore it through guards.

use super::*;

/// Restores the test thread's full mask even when an assertion unwinds.
struct OriginalMask(libc::sigset_t);

impl OriginalMask {
    /// Captures the incoming mask independently of the production callback guard.
    fn new() -> Self {
        let mut mask = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut mask) }, 0);
        Self(mask)
    }
}

impl Drop for OriginalMask {
    /// Restores all test-owned mask changes on the same thread.
    fn drop(&mut self) {
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut()); }
    }
}

/// Reads membership without changing the calling thread's signal mask.
fn blocked(signal: libc::c_int) -> bool {
    unsafe {
        let mut mask = std::mem::zeroed();
        assert_eq!(libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut mask), 0);
        libc::sigismember(&mask, signal) == 1
    }
}

/// Nested deferrals preserve one original child mask and restore the parent's masks in order.
#[test]
fn nested_deferrals_keep_child_signal_inheritance() {
    let _original = OriginalMask::new();
    let baseline = (blocked(libc::SIGTERM), blocked(libc::SIGINT));
    let outer = Deferral::new();
    assert!(blocked(libc::SIGTERM) && blocked(libc::SIGINT));
    let inner = Deferral::new();
    unsafe {
        let mut custom = std::mem::zeroed();
        libc::sigemptyset(&mut custom);
        libc::sigaddset(&mut custom, libc::SIGUSR1);
        assert_eq!(libc::pthread_sigmask(libc::SIG_BLOCK, &custom, std::ptr::null_mut()), 0);
    }
    {
        let _permit = SpawnPermit::new();
        assert_eq!((blocked(libc::SIGTERM), blocked(libc::SIGINT)), baseline);
        assert!(blocked(libc::SIGUSR1), "process creation changed an application mask bit");
    }
    assert!(blocked(libc::SIGUSR1));
    assert!(blocked(libc::SIGTERM) && blocked(libc::SIGINT));
    drop(inner);
    assert!(blocked(libc::SIGTERM) && blocked(libc::SIGINT));
    assert!(blocked(libc::SIGUSR1), "nested callback discarded an application mask bit");
    drop(outer);
    assert_eq!((blocked(libc::SIGTERM), blocked(libc::SIGINT)), baseline);
    assert!(blocked(libc::SIGUSR1), "outer callback discarded an application mask bit");
}
