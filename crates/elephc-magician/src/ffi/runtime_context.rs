//! Purpose:
//! Keeps the native arena pointer authoritative during one Rust eval activation.
//!
//! Called from:
//! - The context-aware eval entry and generated C value-wrapper gateways.
//!
//! Key details:
//! - This is thread-local, nested, and borrowed; it never owns or selects an arena.
//! - Rust's use of callee-saved registers cannot change the captured pointer.

use std::cell::Cell;
use std::ffi::c_void;

thread_local! {
    static ACTIVE: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}

/// Restores the previous activation, including when Rust unwinds.
pub(super) struct Activation(*mut c_void);

impl Activation {
    /// Borrows the arena supplied explicitly by the native caller.
    pub(super) fn enter(context: *mut c_void) -> Self {
        Self(ACTIVE.with(|active| active.replace(context)))
    }
}

impl Drop for Activation {
    /// Restores an outer eval activation without freeing either arena.
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(self.0));
    }
}

/// Returns only the arena captured by the current thread's live eval activation.
#[no_mangle]
pub extern "C" fn __elephc_eval_active_runtime_context() -> *mut c_void {
    ACTIVE.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nested entries and panic cleanup restore the outer borrowed pointer.
    #[test]
    fn eval_runtime_context_activation_is_nested_and_unwind_safe() {
        let mut outer = 0_u8;
        let mut inner = 0_u8;
        let outer_ptr = std::ptr::from_mut(&mut outer).cast();
        let inner_ptr = std::ptr::from_mut(&mut inner).cast();
        assert!(__elephc_eval_active_runtime_context().is_null());
        let outer_guard = Activation::enter(outer_ptr);
        let result = std::panic::catch_unwind(|| {
            let _inner_guard = Activation::enter(inner_ptr);
            assert_eq!(__elephc_eval_active_runtime_context(), inner_ptr);
            panic!("activation cleanup probe");
        });
        assert!(result.is_err());
        assert_eq!(__elephc_eval_active_runtime_context(), outer_ptr);
        std::thread::spawn(|| assert!(__elephc_eval_active_runtime_context().is_null()))
            .join().unwrap();
        drop(outer_guard);
        assert!(__elephc_eval_active_runtime_context().is_null());
    }
}
