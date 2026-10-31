//! Purpose:
//! Defines the frontend's diagnostic value types without terminal-reporting dependencies.
//!
//! Called from:
//! - Lexer, parser, compiler passes, and the root crate's reporting module.
//!
//! Key details:
//! - The root crate adds `report`/`report_warning` on top of these shared values.
//! - Diagnostics keep optional file data and related errors for include-aware reporting.

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
    AotLiteralRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Severity of a compiler diagnostic that does not stop compilation.
pub enum WarningKind {
    /// Ordinary compiler warning.
    #[default]
    Warning,
    /// PHP deprecation notice raised while compiling a constant expression.
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

    /// Sets the file name and returns self for chaining.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Combines a vector of errors into a single error, treating the first as primary and the rest as related.
    ///
    /// # Panics
    /// Panics if `errors` is empty.
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
