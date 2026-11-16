//! Purpose:
//! A cell for the engine's process-wide state.
//!
//! Called from:
//! - Engine modules holding registries (interned strings, resources, call
//!   bookkeeping) that are not part of the C-visible globals.
//!
//! Key details:
//! - The engine is single-threaded by contract: a hosted extension runs on the
//!   program's one thread, as a php-cli request does. The cell therefore
//!   hands out plain references; callers must not hold one across a call that
//!   may re-enter the same registry.

use std::cell::UnsafeCell;

pub struct Global<T>(UnsafeCell<T>);

// SAFETY: the engine is only ever entered from one thread (see module docs).
unsafe impl<T> Sync for Global<T> {}

impl<T> Global<T> {
    /// Creates a cell holding the initial state; usable in statics.
    pub const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    /// The state itself.
    ///
    /// # Safety
    /// No other reference obtained from this cell may be live.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get(&self) -> &mut T {
        &mut *self.0.get()
    }
}
