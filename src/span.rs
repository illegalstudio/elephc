//! Purpose:
//! Defines the source-position value threaded through tokens, AST nodes, diagnostics, and rewrites.
//! Carries one-based line and column coordinates from lexer output into later passes.
//!
//! Called from:
//! - `crate::lexer`, `crate::parser`, and diagnostic-producing compiler passes.
//!
//! Key details:
//! - Spans describe the original PHP source location and should be preserved through AST rewrites.

#[derive(Debug, Clone, Copy)]
/// Source position span for AST nodes.
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    /// Creates a new span from one-based line and column coordinates.
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

    /// Creates a dummy span at line 0, column 0.
    /// Used for synthetic or generated nodes without a source location.
    pub fn dummy() -> Self {
        Self { line: 0, col: 0 }
    }
}
