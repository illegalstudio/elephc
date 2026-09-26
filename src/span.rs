//! Purpose:
//! Defines the source-position value threaded through tokens, AST nodes, diagnostics, and rewrites.
//! Carries one-based line/column coordinates and a source identity from lexer output into later passes.
//!
//! Called from:
//! - `crate::lexer`, `crate::parser`, and diagnostic-producing compiler passes.
//!
//! Key details:
//! - Spans describe the original PHP source location and should be preserved through AST rewrites.
//! - `end_line`/`end_col` are the EXCLUSIVE end position (the character after the
//!   spanned text). A span whose end equals its start is a point span: the extent
//!   is unknown and only the start position is meaningful.
//! - Spans stay 16 bytes: included-file identity is packed into unused high bits of `end_col`,
//!   preserving the AST and parser-frame size while distinguishing equal source coordinates.

/// The first line number handed out to synthetically built nodes.
///
/// Far above any plausible source file, so a synthetic span can never equal one the lexer
/// produced. That matters because spans are used as MAP KEYS, not just as coordinates.
const SYNTHETIC_LINE_BASE: u32 = 1_000_000;

/// Counts synthetic lines handed out this process. A compile is one process, so a given
/// program always gets the same numbering.
static NEXT_SYNTHETIC_LINE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Assigns an in-process identity to each separately parsed included source file.
const PACKED_SOURCE_SPAN: u32 = 1 << 31;
const SOURCE_ID_MASK: u32 = 0x7fff;
const PACKED_END_COL_MASK: u32 = 0xffff;

std::thread_local! {
    static NEXT_SOURCE_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Source position span for AST nodes.
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    /// The upper bits carry source identity for included files; use [`Span::end_column`]
    /// to read the coordinate without the packed identity.
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

    /// Creates a point span associated with one physical source file.
    pub fn new_in_source(line: u32, col: u32, source_id: u32) -> Self {
        Self {
            line,
            col,
            end_line: line,
            end_col: Self::pack_end_column(col, source_id),
        }
    }

    /// Creates a default-source span from one-based start and exclusive end positions.
    pub fn with_end(line: u32, col: u32, end_line: u32, end_col: u32) -> Self {
        Self {
            line,
            col,
            end_line,
            end_col,
        }
    }

    /// Creates a span extent while preserving the identity attached to its token start.
    pub fn with_end_from(start: Span, end: Span) -> Self {
        let mut span = Self::with_end(start.line, start.col, end.end_line, end.end_column());
        span.end_col = Self::pack_end_column(end.end_column(), start.source_id());
        span
    }

    /// Returns a fresh source identity for a separately parsed included file.
    pub fn fresh_source_id() -> u32 {
        NEXT_SOURCE_ID.with(|next| {
            let id = next.get();
            assert!(id <= SOURCE_ID_MASK, "too many included source files in one compile");
            next.set(id + 1);
            id
        })
    }

    /// Resets source identities at the beginning of a new include-resolution unit.
    pub fn reset_source_ids() {
        NEXT_SOURCE_ID.with(|next| next.set(1));
    }

    /// Returns the source end column without the included-file identity bits.
    pub fn end_column(self) -> u32 {
        if self.end_col & PACKED_SOURCE_SPAN == 0 {
            self.end_col
        } else {
            self.end_col & PACKED_END_COL_MASK
        }
    }

    /// Returns the included-file identity, or zero for the root/default source.
    pub fn source_id(self) -> u32 {
        if self.end_col & PACKED_SOURCE_SPAN == 0 {
            0
        } else {
            (self.end_col >> 16) & SOURCE_ID_MASK
        }
    }

    /// Packs an included-file source identity with its end column, asserting both fit.
    fn pack_end_column(end_col: u32, source_id: u32) -> u32 {
        if source_id == 0 {
            return end_col;
        }
        assert!(source_id <= SOURCE_ID_MASK, "source identity exceeds packed span range");
        assert!(end_col <= PACKED_END_COL_MASK, "included-source column exceeds packed span range");
        PACKED_SOURCE_SPAN | (source_id << 16) | end_col
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
        self.end_line > self.line || (self.end_line == self.line && self.end_column() > self.col)
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
        let start_source_id = if (other.line, other.col) < (self.line, self.col) {
            other.source_id()
        } else {
            self.source_id()
        };
        let (end_line, end_col) =
            if (other.end_line, other.end_column()) > (self.end_line, self.end_column()) {
                (other.end_line, other.end_column())
            } else {
                (self.end_line, self.end_column())
            };
        Span {
            line,
            col,
            end_line,
            end_col: Self::pack_end_column(end_col, start_source_id),
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

    /// Equal source coordinates from included files remain distinct map keys.
    #[test]
    fn included_source_identity_is_packed_without_changing_coordinates() {
        let first = Span::new_in_source(4, 10, 7);
        let second = Span::new_in_source(4, 10, 8);
        assert_ne!(first, second);
        assert_eq!(first.source_id(), 7);
        assert_eq!(first.end_column(), 10);

        let extended = Span::with_end_from(first, Span::new_in_source(4, 20, 7));
        assert_eq!(extended.source_id(), 7);
        assert_eq!(extended.end_column(), 20);
        assert!(extended.has_extent());
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
