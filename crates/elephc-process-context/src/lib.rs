//! Purpose:
//! Defers worker termination inside PHP while preserving child process signal inheritance.
//!
//! Called from:
//! - Web application callback boundaries and native/eval process creation helpers.
//!
//! Key details:
//! - Stable, non-inlined C entries share one thread-local context across linked bridge archives.
//! - Process creation removes only worker-added termination blocking; other mask changes survive.
//! - A fork child leaves the callback deferral context before returning to PHP.

use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Cooperative worker disposition that must not remain installed in a fork child.
static WORKER_HANDLER: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct State {
    baseline: libc::sigset_t,
    depth: usize,
}

thread_local! {
    static STATE: Cell<Option<State>> = const { Cell::new(None) };
}

/// Owns the incoming termination mask and stays on the thread whose mask it changes.
pub struct Deferral(libc::sigset_t, PhantomData<std::rc::Rc<()>>);

impl Deferral {
    /// Blocks termination until the callback returns, retaining the outermost baseline for children.
    pub fn new() -> Self {
        let mut previous = unsafe { std::mem::zeroed() };
        let result = unsafe { __elephc_process_defer_begin(&mut previous) };
        assert_eq!(result, 0, "cannot defer application termination signals");
        Self(previous, PhantomData)
    }
}

impl Default for Deferral {
    /// Creates a new callback deferral guard.
    fn default() -> Self { Self::new() }
}

impl Drop for Deferral {
    /// Restores termination blocking while preserving application changes to other signals.
    fn drop(&mut self) { unsafe { __elephc_process_defer_end(&self.0); } }
}

/// Temporarily exposes the pre-callback signal mask on the process-creating thread.
pub struct SpawnPermit(Option<libc::sigset_t>, PhantomData<std::rc::Rc<()>>);

impl SpawnPermit {
    /// Removes worker-added termination blocking while preserving other application mask changes.
    pub fn new() -> Self {
        let mut previous = unsafe { std::mem::zeroed() };
        let result = unsafe { __elephc_process_spawn_begin(&mut previous) };
        assert!(result >= 0, "cannot restore child process signal mask");
        Self((result == 1).then_some(previous), PhantomData)
    }

    /// Reports whether process creation is crossing an active worker callback boundary.
    pub fn is_active(&self) -> bool { self.0.is_some() }

    /// Keeps a fork child unmasked and removes its inherited callback deferral registration.
    pub fn in_child(&mut self) {
        __elephc_process_spawn_child();
        self.0 = None;
    }
}

impl Default for SpawnPermit {
    /// Creates a process creation permit.
    fn default() -> Self { Self::new() }
}

impl Drop for SpawnPermit {
    /// Restores the parent's deferred mask after its process creation operation finishes.
    fn drop(&mut self) {
        if let Some(previous) = &self.0 {
            unsafe { __elephc_process_spawn_end(previous); }
        }
    }
}

/// Registers a callback deferral and writes its incoming mask to caller-owned storage.
///
/// # Safety
/// `previous` must point to writable native signal-set storage.
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn __elephc_process_defer_begin(previous: *mut libc::sigset_t) -> libc::c_int {
    let mut blocked = std::mem::zeroed();
    libc::sigemptyset(&mut blocked);
    libc::sigaddset(&mut blocked, libc::SIGINT);
    libc::sigaddset(&mut blocked, libc::SIGTERM);
    let result = libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, previous);
    if result == 0 {
        STATE.with(|cell| {
            let state = cell.get().map_or(State { baseline: *previous, depth: 1 }, |state| {
                State { depth: state.depth + 1, ..state }
            });
            cell.set(Some(state));
        });
    }
    result
}

/// Unregisters a callback before pending termination signals can run its restored handler.
///
/// # Safety
/// `previous` must point to the matching begin call's saved signal mask.
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn __elephc_process_defer_end(previous: *const libc::sigset_t) {
    STATE.with(|cell| {
        cell.set(cell.get().and_then(|state| {
            (state.depth > 1).then(|| State { depth: state.depth - 1, ..state })
        }));
    });
    let mut current = std::mem::zeroed();
    if libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut current) != 0 { return; }
    for signal in [libc::SIGINT, libc::SIGTERM] {
        if libc::sigismember(previous, signal) == 1 {
            libc::sigaddset(&mut current, signal);
        } else {
            libc::sigdelset(&mut current, signal);
        }
    }
    libc::pthread_sigmask(libc::SIG_SETMASK, &current, std::ptr::null_mut());
}

/// Saves the current mask and removes only the outer callback's added termination blocking.
///
/// Returns one when active, zero outside a callback, or a negative pthread error.
///
/// # Safety
/// `previous` must point to writable native signal-set storage.
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn __elephc_process_spawn_begin(previous: *mut libc::sigset_t) -> libc::c_int {
    STATE.with(|cell| match cell.get() {
        None => 0,
        Some(state) => {
            let result = libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), previous);
            if result != 0 { return -result; }
            let mut allowed = *previous;
            for signal in [libc::SIGINT, libc::SIGTERM] {
                if libc::sigismember(&state.baseline, signal) == 0 {
                    libc::sigdelset(&mut allowed, signal);
                }
            }
            let result = libc::pthread_sigmask(libc::SIG_SETMASK, &allowed, std::ptr::null_mut());
            if result == 0 { 1 } else { -result }
        }
    })
}

/// Restores the parent's callback mask after a fork, spawn, or failed exec.
///
/// # Safety
/// `previous` must point to the matching active spawn-begin call's saved mask.
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn __elephc_process_spawn_end(previous: *const libc::sigset_t) {
    libc::pthread_sigmask(libc::SIG_SETMASK, previous, std::ptr::null_mut());
}

/// Registers the cooperative disposition installed by the web worker after fork.
#[no_mangle]
#[inline(never)]
pub extern "C" fn __elephc_process_register_worker_handler(handler: libc::sighandler_t) {
    WORKER_HANDLER.store(handler, Ordering::Relaxed);
}

/// Clears callback state and removes only inherited cooperative worker dispositions.
///
/// PCNTL calls this with signals blocked before restoring the child's allowed mask.
#[no_mangle]
#[inline(never)]
pub extern "C" fn __elephc_process_spawn_child() {
    let handler = WORKER_HANDLER.load(Ordering::Relaxed);
    if handler != 0 {
        unsafe {
            for signal in [libc::SIGINT, libc::SIGTERM] {
                let mut action: libc::sigaction = std::mem::zeroed();
                if libc::sigaction(signal, std::ptr::null(), &mut action) == 0
                    && action.sa_sigaction == handler
                {
                    action.sa_sigaction = libc::SIG_DFL;
                    action.sa_flags = 0;
                    libc::sigemptyset(&mut action.sa_mask);
                    libc::sigaction(signal, &action, std::ptr::null_mut());
                }
            }
        }
    }
    STATE.with(|cell| cell.set(None));
}

#[cfg(test)]
mod tests;
