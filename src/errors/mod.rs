//! Purpose:
//! Defines compiler diagnostic types and public reporting entry points.
//! Carries error, warning, file, and source-span data from all frontend and analysis passes.
//!
//! Called from:
//! - Compiler passes that construct diagnostics and `crate::pipeline::compile()` when reporting them.
//!
//! Key details:
//! - Diagnostics keep optional file data so included-file errors can point at their real source.

mod report;

use crate::span::Span;

#[derive(Debug, Clone)]
/// Compile error.
pub struct CompileError {
    pub span: Span,
    pub file: Option<String>,
    pub message: String,
    pub related: Vec<CompileError>,
    pub kind: CompileErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// The class of a compile error, for consumers that must react to one class specifically.
pub enum CompileErrorKind {
    /// Every diagnostic without a more specific class.
    #[default]
    General,
    /// A builtin argument that AOT compilation needs as a compile-time literal was not one.
    ///
    /// The backend reads such an argument while lowering the call (a result-shape flag, a
    /// constant or class name), so no spelling of the call can defer it to run time.
    AotLiteralRequired,
    /// A selected semantic guarantee rejects this operation through every invocation spelling.
    SemanticViolation(elephc_semantics::StrictFeature),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Severity of a compiler diagnostic that does not stop compilation.
pub enum WarningKind {
    /// Ordinary compiler warning.
    #[default]
    Warning,
    /// PHP deprecation notice raised at compile time (an implicitly nullable parameter type).
    Deprecation,
}

#[derive(Debug, Clone)]
/// Compile warning.
pub struct CompileWarning {
    pub span: Span,
    pub message: String,
    pub kind: WarningKind,
}

impl CompileError {
    /// Creates a new error with the given span and message, with no file and no related errors.
    pub fn new(span: Span, message: &str) -> Self {
        Self {
            span,
            file: None,
            message: message.to_string(),
            related: Vec::new(),
            kind: CompileErrorKind::General,
        }
    }

    /// Creates the error for a builtin argument that must be a compile-time literal in AOT mode.
    pub fn aot_literal_required(span: Span, message: &str) -> Self {
        Self {
            kind: CompileErrorKind::AotLiteralRequired,
            ..Self::new(span, message)
        }
    }

    /// Retains the owning guarantee so fallback dispatch cannot erase a semantic rejection.
    pub fn semantic_violation(span: Span, feature: elephc_semantics::StrictFeature, message: &str) -> Self {
        Self { kind: CompileErrorKind::SemanticViolation(feature), ..Self::new(span, message) }
    }

    /// Sets the file name and returns self for chaining.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Combines a vector of errors into a single error, treating the first as primary and the rest as related.
    ///
    /// # Panics
    /// Panics if `errors` is empty. Callers must guarantee at least one error; the
    /// explicit assert turns a misuse into a clear message instead of an opaque
    /// `Vec::remove(0)` index-out-of-bounds panic.
    pub fn from_many(mut errors: Vec<CompileError>) -> Self {
        assert!(!errors.is_empty(), "from_many requires at least one error");
        let mut first = errors.remove(0);
        first.related = errors;
        first
    }

    /// Returns all errors including related ones as a flat vector, with the primary error first.
    pub fn flatten(&self) -> Vec<CompileError> {
        let mut first = self.clone();
        first.related.clear();
        let mut all = vec![first];
        for related in &self.related {
            all.extend(related.flatten());
        }
        all
    }
}

impl CompileWarning {
    /// Creates a new warning with the given span and message.
    pub fn new(span: Span, message: &str) -> Self {
        Self {
            span,
            message: message.to_string(),
            kind: WarningKind::Warning,
        }
    }

    /// Creates a deprecation diagnostic with the given span and message.
    pub fn deprecation(span: Span, message: &str) -> Self {
        Self {
            span,
            message: message.to_string(),
            kind: WarningKind::Deprecation,
        }
    }
}

impl std::error::Error for CompileError {}

impl std::fmt::Display for CompileError {
    /// Formats this value for display or debug output.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.file, self.span.line > 0) {
            (Some(file), true) => {
                write!(f, "[{}:{}:{}] {}", file, self.span.line, self.span.col, self.message)
            }
            (Some(file), false) => write!(f, "[{}] {}", file, self.message),
            (None, true) => write!(f, "[{}:{}] {}", self.span.line, self.span.col, self.message),
            (None, false) => write!(f, "{}", self.message),
        }
    }
}

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
