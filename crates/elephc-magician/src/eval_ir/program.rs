//! Purpose:
//! Defines parsed EvalIR programs and source-location metadata.
//!
//! Called from:
//! - Parser entry points, declaration metadata, Reflection, and interpreter execution.
//!
//! Key details:
//! - Source offsets and file/line ranges remain syntax metadata, not runtime cells.

use super::*;

/// Parsed eval fragment lowered into dynamic by-name statements.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalProgram {
    source_len: usize,
    statements: Vec<EvalStmt>,
}

impl EvalProgram {
    /// Creates an EvalIR program for a source fragment and statement list.
    pub fn new(source_len: usize, statements: Vec<EvalStmt>) -> Self {
        Self {
            source_len,
            statements,
        }
    }

    /// Returns the byte length of the parsed eval fragment.
    pub const fn source_len(&self) -> usize {
        self.source_len
    }

    /// Returns the ordered EvalIR statements in source order.
    pub fn statements(&self) -> &[EvalStmt] {
        &self.statements
    }

    /// Consumes the program and returns its statement list.
    pub fn into_statements(self) -> Vec<EvalStmt> {
        self.statements
    }
}

/// One source range inside the current eval fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalSourceLocation {
    start_line: i64,
    end_line: i64,
}

impl EvalSourceLocation {
    /// Creates a source range using one-based eval-fragment line numbers.
    pub const fn new(start_line: i64, end_line: i64) -> Self {
        Self {
            start_line,
            end_line,
        }
    }

    /// Creates a single-line source range.
    pub const fn single_line(line: i64) -> Self {
        Self::new(line, line)
    }

    /// Returns the one-based line where the declaration starts.
    pub const fn start_line(&self) -> i64 {
        self.start_line
    }

    /// Returns the one-based line where the declaration ends.
    pub const fn end_line(&self) -> i64 {
        self.end_line
    }
}
