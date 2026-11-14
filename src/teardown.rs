//! Releasing build-wide structures when the process is about to exit anyway.
//!
//! The CLI compiles one program and exits. Dropping the codegen state at the end of that build
//! walks millions of heap nodes (types, signatures, the module copy the shared state keeps, the
//! data section) only to hand pages back to an allocator the exit returns to the system at once:
//! about 7 s of the Symfony demo's single-core build. The CLI entry declares that the process ends
//! with the compile ([`process_exits_after_compile`]); [`release`] then forgets the value instead
//! of dropping it. A library caller (tests, the REPL, `elephc test`) compiles many programs in one
//! process and keeps the ordinary drop.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static EXITS_AFTER_COMPILE: AtomicBool = AtomicBool::new(false);
static FORGOTTEN: AtomicUsize = AtomicUsize::new(0);

/// Declares that this process exits once the current compile ends (the `elephc` CLI's compile
/// command), so build-wide structures need not be torn down.
pub fn process_exits_after_compile() {
    EXITS_AFTER_COMPILE.store(true, Ordering::Relaxed);
}

/// Drops `value`, or forgets it when the process exits after this compile.
pub(crate) fn release<T>(value: T) {
    if EXITS_AFTER_COMPILE.load(Ordering::Relaxed) {
        FORGOTTEN.fetch_add(1, Ordering::Relaxed);
        std::mem::forget(value);
    } else {
        drop(value);
    }
}

/// How many values [`release`] forgot in this process (the regression tests' counter).
pub fn forgotten_count() -> usize {
    FORGOTTEN.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A library caller keeps the ordinary drop: the value's destructor runs and nothing is
    /// counted as forgotten.
    #[test]
    fn a_library_caller_drops_what_it_releases() {
        if EXITS_AFTER_COMPILE.load(Ordering::Relaxed) {
            return;
        }
        let shared = Arc::new(());
        let before = forgotten_count();
        release(Arc::clone(&shared));
        assert_eq!(Arc::strong_count(&shared), 1);
        assert_eq!(forgotten_count(), before);
    }
}
