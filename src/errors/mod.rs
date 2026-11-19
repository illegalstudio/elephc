//! Purpose:
//! Re-exports the syntax crate's diagnostic values and adds terminal reporting.
//!
//! Called from:
//! - Every compiler pass that constructs diagnostics and `crate::pipeline::compile()`.
//!
//! Key details:
//! - The value types live in Chopper's syntax module so the tokenizer bridge links no reporting stack.
//! - Reporting keeps optional file data so included-file errors can point at their real source.

pub use elephc_chopper::syntax::errors::{
    CompileError, CompileErrorKind, CompileWarning, WarningKind,
};

mod report;

thread_local! {
    /// Warnings raised by per-file frontend passes whose results carry no diagnostics channel.
    /// Drained by the pipeline through [`take_deferred_warnings`].
    static DEFERRED_WARNINGS: std::cell::RefCell<Vec<CompileWarning>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Queues a warning raised while a source file is prepared, for the pipeline to report.
pub fn defer_warning(warning: CompileWarning) {
    DEFERRED_WARNINGS.with(|warnings| warnings.borrow_mut().push(warning));
}

/// Takes every queued warning in the order raised, once each.
///
/// The resolver parses an included file once for discovery and once for expansion, and each
/// parse lexes it under a fresh source identity, so the span alone differs between the two.
/// A repeat is therefore keyed on the file's canonical path (the identity when no path was
/// registered), line, column and message, and dropped.
pub fn take_deferred_warnings() -> Vec<CompileWarning> {
    let queued = DEFERRED_WARNINGS.with(|warnings| std::mem::take(&mut *warnings.borrow_mut()));
    let mut seen = std::collections::HashSet::new();
    queued
        .into_iter()
        .filter(|warning| {
            let origin = match warning.span.source_path() {
                Some(path) => Ok(std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| std::path::PathBuf::from(path))),
                None => Err(warning.span.source_id()),
            };
            seen.insert((origin, warning.span.line, warning.span.col, warning.message.clone()))
        })
        .collect()
}

/// Prints the error message to stderr with file:line:col formatting when available.
pub fn report(error: &CompileError) {
    report::print_error(error);
}

/// Prints the warning message to stderr with file:line:col formatting when available.
pub fn report_warning(warning: &CompileWarning) {
    report::print_warning(warning);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    /// Verifies `from_many` panics with an explicit precondition message when given an
    /// empty error vector, instead of the opaque `Vec::remove(0)` index-out-of-bounds panic.
    #[test]
    #[should_panic(expected = "from_many requires at least one error")]
    fn from_many_empty_vec_panics_with_clear_message() {
        let _ = CompileError::from_many(Vec::new());
    }

    /// Verifies `from_many` keeps the first error as primary and attaches the rest as related.
    #[test]
    fn from_many_promotes_first_and_keeps_rest_related() {
        let errors = vec![
            CompileError::new(Span::new(1, 1), "first"),
            CompileError::new(Span::new(2, 2), "second"),
        ];
        let combined = CompileError::from_many(errors);
        assert_eq!(combined.message, "first");
        assert_eq!(combined.related.len(), 1);
        assert_eq!(combined.related[0].message, "second");
    }
}
