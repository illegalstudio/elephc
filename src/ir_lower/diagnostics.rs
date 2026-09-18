//! Purpose:
//! Collects AST-to-EIR lowering refusals so an unsupported shape fails compilation
//! with a source diagnostic instead of emitting unsound code.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` and the lowering sites that refuse a shape.
//!
//! Key details:
//! - The sink is thread-local because every lowering run owns one thread, and the
//!   codegen test harness lowers several programs in parallel.
//! - `begin_collection` clears the sink, so a previous refusal can never leak into
//!   the next lowering run on the same thread.
//! - Collection is ROLLBACK-safe: a speculative region that is lowered and then thrown
//!   away (`stmt::repr_fixpoint`) must not leave a provisional refusal behind, because
//!   the guaranteed final lowering of that same region reports it again.

use std::cell::RefCell;

use crate::errors::CompileError;
use crate::span::Span;

thread_local! {
    /// Refusals recorded while lowering the current program, in emission order.
    static REFUSALS: RefCell<Vec<CompileError>> = const { RefCell::new(Vec::new()) };
}

/// Clears the sink before a lowering run so no earlier run's refusal is reported.
pub(crate) fn begin_collection() {
    REFUSALS.with(|refusals| refusals.borrow_mut().clear());
}

/// Records one unsupported lowering shape at `span`.
///
/// Lowering continues after the call so the rest of the program is still walked and
/// the emitted EIR stays well formed; `take_first` turns the record into the compile
/// error `program::lower` returns before the module is handed to codegen.
pub(crate) fn refuse(span: Span, message: &str) {
    REFUSALS.with(|refusals| {
        refusals
            .borrow_mut()
            .push(CompileError::new(span, message));
    });
}

/// Returns the number of refusals recorded so far, which is a rollback mark.
pub(crate) fn mark() -> usize {
    REFUSALS.with(|refusals| refusals.borrow().len())
}

/// Discards every refusal recorded after `mark`, undoing a speculative lowering.
///
/// The region a rollback discards is always lowered again for real, so a refusal inside it is
/// recorded again by that second pass. Keeping the provisional record instead would report a
/// refusal for code the module never contains, and would report the FIRST (speculative) one
/// rather than the final diagnostic.
pub(crate) fn rollback_to(mark: usize) {
    REFUSALS.with(|refusals| {
        let mut refusals = refusals.borrow_mut();
        if mark < refusals.len() {
            refusals.truncate(mark);
        }
    });
}

/// Removes and returns the earliest recorded refusal, if any.
pub(crate) fn take_first() -> Option<CompileError> {
    REFUSALS.with(|refusals| {
        let mut refusals = refusals.borrow_mut();
        if refusals.is_empty() {
            return None;
        }
        Some(refusals.remove(0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A span is only needed to build a record; these tests care about the sink's bookkeeping.
    fn span() -> Span {
        Span::new(1, 1)
    }

    /// A rollback discards exactly the records made after its mark and keeps the earlier ones.
    #[test]
    fn rollback_discards_only_records_made_after_the_mark() {
        begin_collection();
        refuse(span(), "kept");
        let mark = mark();
        refuse(span(), "speculative");
        refuse(span(), "also speculative");
        rollback_to(mark);
        assert_eq!(take_first().map(|error| error.message), Some("kept".to_string()));
        assert!(take_first().is_none(), "the speculative records were discarded");
    }

    /// A rollback mark taken before anything was recorded empties the sink again.
    #[test]
    fn rollback_to_an_empty_mark_reports_no_refusal() {
        begin_collection();
        let mark = mark();
        refuse(span(), "speculative only");
        rollback_to(mark);
        assert!(take_first().is_none());
    }

    /// Starting a run clears whatever an earlier run on this thread left behind.
    #[test]
    fn a_new_collection_cannot_report_an_earlier_run_s_refusal() {
        begin_collection();
        refuse(span(), "previous run");
        begin_collection();
        assert!(take_first().is_none());
        refuse(span(), "current run");
        assert_eq!(
            take_first().map(|error| error.message),
            Some("current run".to_string())
        );
    }

    /// Each lowering thread owns its own sink, so parallel test lowerings cannot cross-report.
    #[test]
    fn refusals_stay_on_the_thread_that_recorded_them() {
        begin_collection();
        refuse(span(), "main thread");
        std::thread::spawn(|| {
            begin_collection();
            assert!(take_first().is_none(), "a fresh thread starts empty");
            refuse(span(), "worker thread");
            assert_eq!(
                take_first().map(|error| error.message),
                Some("worker thread".to_string())
            );
        })
        .join()
        .expect("the worker thread completes");
        assert_eq!(
            take_first().map(|error| error.message),
            Some("main thread".to_string())
        );
    }
}
