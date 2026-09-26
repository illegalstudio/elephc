//! Purpose:
//! Measures rejected encoder units, including failures inside replacement fallback calls.
//!
//! Called from:
//! - Shared replacement emission and counted conversion operations.
//!
//! Key details:
//! - The thread-local meter is instrumentation, not PHP request state.
//! - Nested measurements include their inner work; panics cannot leave an active scope.
//! - Only explicit request operations add a measurement to PHP's illegal-character count.

use std::cell::Cell;

thread_local! {
    static REJECTED_UNITS: Cell<u64> = const { Cell::new(0) };
}

/// Records one rejected original or replacement codepoint on the current thread.
pub(super) fn rejected() {
    REJECTED_UNITS.with(|counter| counter.set(counter.get().wrapping_add(1)));
}

/// Measures encoder rejections produced by this call without resetting any enclosing measurement.
pub(crate) fn measure<T>(operation: impl FnOnce() -> T) -> (T, u64) {
    let before = REJECTED_UNITS.with(Cell::get);
    let result = operation();
    let errors = REJECTED_UNITS.with(|counter| counter.get().wrapping_sub(before));
    (result, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies nested, concurrent, and unwound calls cannot corrupt later measurements.
    #[test]
    fn rejection_measurements_are_nested_and_thread_local() {
        let ((inner, other), total) = measure(|| {
            rejected();
            let (_, inner) = measure(|| { rejected(); rejected(); });
            let other = std::thread::spawn(|| measure(|| { rejected(); rejected(); rejected(); }).1).join().unwrap();
            (inner, other)
        });
        assert_eq!((inner, other, total), (2, 3, 3));
        let _ = std::panic::catch_unwind(|| measure(|| { rejected(); panic!("measured failure"); }));
        assert_eq!(measure(|| ()).1, 0);
    }
}
