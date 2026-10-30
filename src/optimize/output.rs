//! Purpose:
//! Scopes PHP host output-handler effects to extension compilation.
//!
//! Called from:
//! - The CLI pipeline and AST effect/exception analysis.
//!
//! Key details:
//! - Standalone builds retain the prior output approximation; chunked native
//!   ob_start callbacks still need callback-aware analysis.
//! - Nested compilations and unwinding restore the caller's profile.

use std::cell::Cell;

thread_local! {
    static HOSTED_OUTPUT: Cell<bool> = const { Cell::new(false) };
}

/// Installs whether output can call an external PHP host handler for one compilation.
pub fn with_hosted_output<R>(enabled: bool, body: impl FnOnce() -> R) -> R {
    HOSTED_OUTPUT.with(|slot| {
        let previous = slot.replace(enabled);
        let _restore = RestoreOutput { slot, previous };
        body()
    })
}

/// Restores the enclosing compilation profile even when a compiler pass unwinds.
struct RestoreOutput<'a> {
    slot: &'a Cell<bool>,
    previous: bool,
}

impl Drop for RestoreOutput<'_> {
    /// Returns the thread-local setting to the caller's profile.
    fn drop(&mut self) {
        self.slot.set(self.previous);
    }
}

/// Returns whether a host output handler can throw into the current program.
pub(super) fn output_may_throw() -> bool {
    HOSTED_OUTPUT.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nested compilations and failed compiler passes cannot change a later output profile.
    #[test]
    fn hosted_output_profile_restores_nested_and_panicking_compilations() {
        assert!(!output_may_throw());
        with_hosted_output(true, || {
            assert!(output_may_throw());
            with_hosted_output(false, || assert!(!output_may_throw()));
            assert!(output_may_throw());
        });
        let failed = std::panic::catch_unwind(|| with_hosted_output(true, || panic!("compiler pass")));
        assert!(failed.is_err());
        assert!(!output_may_throw());
    }
}
