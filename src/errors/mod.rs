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
}

#[derive(Debug, Clone)]
/// Compile warning.
pub struct CompileWarning {
    pub span: Span,
    pub message: String,
}

impl CompileError {
    /// Creates a new error with the given span and message, with no file and no related errors.
    pub fn new(span: Span, message: &str) -> Self {
        Self {
            span,
            file: None,
            message: message.to_string(),
            related: Vec::new(),
        }
    }

    /// Sets the file name and returns self for chaining.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Combines a vector of errors into a single error, treating the first as primary and the rest as related.
    pub fn from_many(mut errors: Vec<CompileError>) -> Self {
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

/// Prints the error message to stderr with file:line:col formatting when available.
pub fn report(error: &CompileError) {
    report::print_error(error);
}

/// Prints the warning message to stderr with file:line:col formatting when available.
pub fn report_warning(warning: &CompileWarning) {
    report::print_warning(warning);
}
