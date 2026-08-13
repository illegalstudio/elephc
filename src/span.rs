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
//! - Coordinates are `u32` to keep `Span` at 16 bytes: it is embedded in every
//!   token and AST node, so its size directly sets the recursive parser's stack
//!   frame growth (a 32-byte span overflowed 2 MiB test-thread stacks).

/// The first line number handed out to synthetically built nodes.
///
/// Far above any plausible source file, so a synthetic span can never equal one the lexer
/// produced. That matters because spans are used as MAP KEYS, not just as coordinates.
const SYNTHETIC_LINE_BASE: u32 = 1_000_000;

/// Counts synthetic lines handed out this process. A compile is one process, so a given
/// program always gets the same numbering.
static NEXT_SYNTHETIC_LINE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Source position span for AST nodes.
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Span {
    /// Creates a point span from one-based line and column coordinates.
    /// The end position equals the start (extent unknown).
    pub fn new(line: u32, col: u32) -> Self {
        Self {
            line,
            col,
            end_line: line,
            end_col: col,
        }
    }

    /// Creates a span from a one-based start position and exclusive end position.
    pub fn with_end(line: u32, col: u32, end_line: u32, end_col: u32) -> Self {
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

    /// A DISTINCT span for one synthetically built node.
    ///
    /// A span is not only a diagnostic coordinate: the checker records every builtin call's
    /// inferred type in `CheckResult::builtin_call_types`, KEYED BY SPAN, and lowering reads it
    /// back. Nodes that all carry `dummy()` therefore share one key, so the map cannot name an
    /// individual call among them and lowering falls back to the builtin's DECLARED return type.
    ///
    /// That fallback used to be a miscompile: six builtins checked as `PhpType::Pointer` or
    /// `PhpType::Callable` while declaring `mixed`, because `TypeSpec` had no variant for
    /// either, so codegen got a boxed cell for a raw descriptor. `TypeSpec::Ptr` and
    /// `TypeSpec::Callable` fixed that at the declaration, and it was measured: with `dummy()`
    /// put back, all 276 PDO tests pass.
    ///
    /// Distinct spans remain because the assertion in `resolve_registry_builtin_result_type`
    /// compares the declared and checked types, and can only do so where the checked one is
    /// findable — under `dummy()` the map is skipped for every prelude call, and the next
    /// mismatched declaration would go unwitnessed there.
    ///
    /// Lines start past `SYNTHETIC_LINE_BASE` so a synthetic node cannot collide with a node
    /// from real source either, and the counter is per-process: one compile is one process, so
    /// the numbering is stable for a given program.
    pub fn synthetic() -> Self {
        let line = SYNTHETIC_LINE_BASE
            + NEXT_SYNTHETIC_LINE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            line,
            col: 1,
            end_line: line,
            end_col: 1,
        }
    }

    /// Does this span point at a place in the program's own source?
    ///
    /// Three things carry a span: real source, `dummy()`, and `synthetic()`. Passes that treat a
    /// node as compiler-generated must ask this rather than testing `line == 0`, which was the
    /// only spelling of "generated" before synthetic spans existed and now answers wrongly for
    /// half of them.
    pub fn is_from_source(self) -> bool {
        self.line != 0 && self.line < SYNTHETIC_LINE_BASE
    }

    /// Can this span single out ONE node?
    ///
    /// A `dummy()` cannot: every node built without a source location shares it, so anything
    /// keyed by it — `builtin_call_types`, `throw_access_sites`, `loop_storage_types`, the
    /// call-type memo in loop-storage stabilisation — hands one node's entry to all the others.
    /// A synthetic span can, which is the whole reason it exists.
    pub fn identifies_a_node(self) -> bool {
        self.line != 0
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

    /// Spans are embedded in every token and AST node and flow through the
    /// recursive-descent parser's stack frames; growing this struct directly
    /// deepens no recursion but fattens every frame, and 2 MiB test threads
    /// overflowed when it doubled to 32 bytes. Keep it at 16.
    #[test]
    fn span_stays_16_bytes() {
        assert_eq!(std::mem::size_of::<Span>(), 16);
    }

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
