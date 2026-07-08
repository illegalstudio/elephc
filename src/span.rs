//! Purpose:
//! Defines the source-position value threaded through tokens, AST nodes, diagnostics, and rewrites.
//! Carries one-based line and column coordinates from lexer output into later passes.
//!
//! Called from:
//! - `crate::lexer`, `crate::parser`, and diagnostic-producing compiler passes.
//!
//! Key details:
//! - Spans describe the original PHP source location and should be preserved through AST rewrites.
//! - `end_line`/`end_col` are the EXCLUSIVE end position (the character after the
//!   spanned text). A span whose end equals its start is a point span: the extent
//!   is unknown and only the start position is meaningful.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Source position span for AST nodes.
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    /// Creates a point span from one-based line and column coordinates.
    /// The end position equals the start (extent unknown).
    pub fn new(line: usize, col: usize) -> Self {
        Self {
            line,
            col,
            end_line: line,
            end_col: col,
        }
    }

    /// Creates a span from a one-based start position and exclusive end position.
    pub fn with_end(line: usize, col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            line,
            col,
            end_line,
            end_col,
        }
    }

    /// Creates a dummy span at line 0, column 0.
    /// Used for synthetic or generated nodes without a source location.
    pub fn dummy() -> Self {
        Self {
            line: 0,
            col: 0,
            end_line: 0,
            end_col: 0,
        }
    }

    /// Returns true when the span covers a real extent (an end position past
    /// the start), as opposed to a point span or a dummy.
    pub fn has_extent(self) -> bool {
        self.end_line > self.line || (self.end_line == self.line && self.end_col > self.col)
    }

    /// Returns the union of two spans: the earlier start and the later end.
    /// A dummy operand (line 0) is ignored so merging with a synthetic child
    /// never drags a real span to 0:0.
    pub fn merge(self, other: Span) -> Span {
        if other.line == 0 {
            return self;
        }
        if self.line == 0 {
            return other;
        }
        let (line, col) = if (other.line, other.col) < (self.line, self.col) {
            (other.line, other.col)
        } else {
            (self.line, self.col)
        };
        let (end_line, end_col) =
            if (other.end_line, other.end_col) > (self.end_line, self.end_col) {
                (other.end_line, other.end_col)
            } else {
                (self.end_line, self.end_col)
            };
        Span {
            line,
            col,
            end_line,
            end_col,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies merge takes the earlier start and later end across lines.
    #[test]
    fn merge_unions_start_and_end() {
        let a = Span::with_end(2, 5, 2, 8);
        let b = Span::with_end(2, 10, 3, 4);
        let merged = a.merge(b);
        assert_eq!(merged, Span::with_end(2, 5, 3, 4));
    }

    /// Verifies merging with a dummy span keeps the real span unchanged in
    /// both operand orders.
    #[test]
    fn merge_ignores_dummy_operands() {
        let real = Span::with_end(4, 1, 4, 9);
        assert_eq!(real.merge(Span::dummy()), real);
        assert_eq!(Span::dummy().merge(real), real);
    }

    /// Verifies a point span reports no extent and a widened span does.
    #[test]
    fn has_extent_distinguishes_point_spans() {
        assert!(!Span::new(3, 7).has_extent());
        assert!(!Span::dummy().has_extent());
        assert!(Span::with_end(3, 7, 3, 12).has_extent());
        assert!(Span::with_end(3, 7, 4, 1).has_extent());
    }
}
