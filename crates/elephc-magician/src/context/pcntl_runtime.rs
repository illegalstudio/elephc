//! Purpose:
//! Owns Magician's process-global PCNTL handler and dispatch state.
//!
//! Called from:
//! - PCNTL signal builtins and eval-context teardown.
//!
//! Key details:
//! - Signal dispositions are process-global, so callable ownership cannot live
//!   in one transient function-frame eval context.
//! - Contexts referenced by active handlers are detached at frame teardown and
//!   reclaimed after their last handler is replaced.

use super::{ElephcEvalContext, EvalPcntlSignalHandler};
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

/// One process-global eval handler plus the context that owns its callable metadata.
#[derive(Clone, Copy)]
pub(crate) struct EvalPcntlSignalEntry {
    /// Context required to invoke an eval callable; null for integer dispositions.
    pub(crate) context: *mut ElephcEvalContext,
    /// Retained callable or native integer disposition.
    pub(crate) handler: EvalPcntlSignalHandler,
}

/// Active-use lease that prevents a detached handler-owner context from being reclaimed.
pub struct EvalPcntlContextLease {
    context: *mut ElephcEvalContext,
    counted: bool,
}

impl EvalPcntlContextLease {
    /// Returns the pinned context pointer for interpreter dispatch.
    pub(crate) fn context_ptr(&self) -> *mut ElephcEvalContext {
        self.context
    }
}

impl Clone for EvalPcntlContextLease {
    /// Adds another active-use reference to the same detached context.
    fn clone(&self) -> Self {
        let counted = if let Ok(mut state) = pcntl_runtime().lock() {
            *state
                .active_contexts
                .entry(self.context as usize)
                .or_default() += 1;
            true
        } else {
            false
        };
        Self {
            context: self.context,
            counted,
        }
    }
}

impl Drop for EvalPcntlContextLease {
    /// Releases the active-use count and finalizes an otherwise unreferenced detached context.
    fn drop(&mut self) {
        if self.counted && end_handler_dispatch(self.context) {
            unsafe {
                crate::ffi::context::drop_eval_context_now(self.context);
            }
        }
    }
}

/// Pointer-free storage used behind the process-global mutex.
#[derive(Clone, Copy)]
struct StoredSignalEntry {
    context: usize,
    disposition: Option<i64>,
    callback: usize,
}

#[derive(Default)]
struct PcntlRuntimeState {
    handlers: HashMap<i32, StoredSignalEntry>,
    detached_contexts: HashSet<usize>,
    active_contexts: HashMap<usize, usize>,
    async_signals: bool,
    dispatching: bool,
    fiber_dispatching: bool,
}

static PCNTL_RUNTIME: OnceLock<Mutex<PcntlRuntimeState>> = OnceLock::new();

/// Returns the process-global eval PCNTL registry.
fn pcntl_runtime() -> &'static Mutex<PcntlRuntimeState> {
    PCNTL_RUNTIME.get_or_init(|| Mutex::new(PcntlRuntimeState::default()))
}

/// Returns the active eval-owned handler for one signal.
pub(crate) fn signal_handler(signal: i32) -> Option<EvalPcntlSignalEntry> {
    let state = pcntl_runtime().lock().ok()?;
    stored_to_entry(*state.handlers.get(&signal)?)
}

/// Returns one handler for dispatch while pinning its owning context until invocation ends.
pub(crate) fn begin_handler_dispatch(signal: i32) -> Option<EvalPcntlSignalEntry> {
    let mut state = pcntl_runtime().lock().ok()?;
    let stored = *state.handlers.get(&signal)?;
    if stored.context != 0 {
        *state.active_contexts.entry(stored.context).or_default() += 1;
    }
    stored_to_entry(stored)
}

/// Pins the detached owner of one handler callable when another eval context invokes it.
pub(crate) fn begin_callable_use(
    callback: RuntimeCellHandle,
    current_context: &ElephcEvalContext,
) -> Option<EvalPcntlContextLease> {
    if let Some(owner) = current_context.pcntl_foreign_callable_owner(callback) {
        return Some(owner.clone());
    }
    let callback = callback.as_ptr() as usize;
    let current_context = current_context as *const ElephcEvalContext as usize;
    let mut state = pcntl_runtime().lock().ok()?;
    let context = state
        .handlers
        .values()
        .find(|entry| {
            entry.callback == callback
                && entry.context != 0
                && entry.context != current_context
        })?
        .context;
    *state.active_contexts.entry(context).or_default() += 1;
    Some(EvalPcntlContextLease {
        context: context as *mut ElephcEvalContext,
        counted: true,
    })
}

/// Returns whether one runtime cell is retained as an eval PCNTL handler callable.
pub(crate) fn is_handler_callable(callback: RuntimeCellHandle) -> bool {
    let callback = callback.as_ptr() as usize;
    pcntl_runtime()
        .lock()
        .map(|state| {
            state
                .handlers
                .values()
                .any(|entry| entry.callback == callback && entry.context != 0)
        })
        .unwrap_or(false)
}

/// Replaces one eval-owned process handler and returns the prior retained entry.
pub(crate) fn replace_signal_handler(
    signal: i32,
    context: *mut ElephcEvalContext,
    handler: EvalPcntlSignalHandler,
) -> Option<EvalPcntlSignalEntry> {
    let stored = match handler {
        EvalPcntlSignalHandler::Disposition(disposition) => StoredSignalEntry {
            context: 0,
            disposition: Some(disposition),
            callback: 0,
        },
        EvalPcntlSignalHandler::Callable(callback) => StoredSignalEntry {
            context: context as usize,
            disposition: None,
            callback: callback.as_ptr() as usize,
        },
    };
    let mut state = pcntl_runtime().lock().ok()?;
    state
        .handlers
        .insert(signal, stored)
        .and_then(stored_to_entry)
}

/// Marks a frame-owned context as detached when an active signal handler still references it.
///
/// Returns true when teardown must defer dropping the context.
pub(crate) fn defer_context_free(context: *mut ElephcEvalContext) -> bool {
    if context.is_null() {
        return false;
    }
    let context = context as usize;
    let Ok(mut state) = pcntl_runtime().lock() else {
        return false;
    };
    if state.handlers.values().any(|entry| entry.context == context) {
        state.detached_contexts.insert(context);
        true
    } else {
        false
    }
}

/// Claims a detached context after replacement removed its last signal-handler reference.
pub(crate) fn take_collectable_context(context: *mut ElephcEvalContext) -> bool {
    if context.is_null() {
        return false;
    }
    let context = context as usize;
    let Ok(mut state) = pcntl_runtime().lock() else {
        return false;
    };
    if state.handlers.values().any(|entry| entry.context == context)
        || state.active_contexts.contains_key(&context)
    {
        return false;
    }
    state.detached_contexts.remove(&context)
}

/// Unpins an invoked handler context and claims it when no registration still references it.
pub(crate) fn end_handler_dispatch(context: *mut ElephcEvalContext) -> bool {
    if context.is_null() {
        return false;
    }
    let context = context as usize;
    let Ok(mut state) = pcntl_runtime().lock() else {
        return false;
    };
    let Some(active) = state.active_contexts.get_mut(&context) else {
        return false;
    };
    if *active > 1 {
        *active -= 1;
        return false;
    }
    state.active_contexts.remove(&context);
    if state.handlers.values().any(|entry| entry.context == context) {
        return false;
    }
    state.detached_contexts.remove(&context)
}

/// Queries or changes automatic eval signal dispatch and returns its previous state.
pub(crate) fn update_async_signals(enabled: Option<bool>) -> bool {
    let Ok(mut state) = pcntl_runtime().lock() else {
        return false;
    };
    let previous = state.async_signals;
    if let Some(enabled) = enabled {
        state.async_signals = enabled;
    }
    previous
}

/// Returns whether automatic eval signal dispatch is enabled process-wide.
pub(crate) fn async_signals() -> bool {
    pcntl_runtime()
        .lock()
        .map(|state| state.async_signals)
        .unwrap_or(false)
}

/// Enters the process-global non-reentrant eval signal-dispatch region.
pub(crate) fn begin_dispatch() -> bool {
    let Ok(mut state) = pcntl_runtime().lock() else {
        return false;
    };
    if state.dispatching {
        false
    } else {
        state.dispatching = true;
        true
    }
}

/// Leaves the process-global eval signal-dispatch region.
pub(crate) fn end_dispatch() {
    if let Ok(mut state) = pcntl_runtime().lock() {
        state.dispatching = false;
    }
}

/// Publishes whether the interpreter is currently invoking a PCNTL handler.
pub(crate) fn set_fiber_dispatching(active: bool) {
    if let Ok(mut state) = pcntl_runtime().lock() {
        state.fiber_dispatching = active;
    }
}

/// Returns whether Magician must reject a Fiber context switch during signal dispatch.
pub(crate) fn fiber_dispatching() -> bool {
    pcntl_runtime()
        .lock()
        .map(|state| state.fiber_dispatching)
        .unwrap_or(false)
}

/// Converts pointer-free registry storage back into an interpreter-facing entry.
fn stored_to_entry(stored: StoredSignalEntry) -> Option<EvalPcntlSignalEntry> {
    let handler = match stored.disposition {
        Some(disposition) => EvalPcntlSignalHandler::Disposition(disposition),
        None if stored.callback != 0 => EvalPcntlSignalHandler::Callable(
            RuntimeCellHandle::from_raw(stored.callback as *mut RuntimeCell),
        ),
        None => return None,
    };
    Some(EvalPcntlSignalEntry {
        context: stored.context as *mut ElephcEvalContext,
        handler,
    })
}
