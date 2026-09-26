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
//! - An included-file end column too wide for the packed field is kept exactly in a
//!   process-wide side table (`OverflowEnds`); the packed word then holds its index.

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
/// How many distinct overflowing `(source identity, end column)` pairs the side table holds:
/// one per value of the packed word's 16 low bits.
const OVERFLOW_CAPACITY: usize = 1 << 16;

/// Exact end columns of included-file spans that do not fit the packed 16-bit field.
///
/// A packed word marks such a span with the flag set and a ZERO identity field — a combination
/// that never occurs otherwise, because included sources are numbered from 1 — and its low 16
/// bits index this table, which holds the real `(source identity, end column)`. Entries are
/// deduplicated, so two spans that end at the same place still compare equal; the table only
/// grows, so an index stays valid for the whole process whatever thread created the span.
///
/// Before this table the end column was asserted to fit (a compiler panic on a valid include,
/// #1292), and then saturated — which put a token starting past column 65535 BEFORE its own end,
/// so `--source-map` dropped its end position (#1306).
struct OverflowEnds {
    entries: Vec<(u32, u32)>,
    index: std::collections::BTreeMap<(u32, u32), u16>,
}

impl OverflowEnds {
    /// Returns the index of `(source_id, end_col)`, adding it while there is room.
    fn intern(&mut self, source_id: u32, end_col: u32, capacity: usize) -> Option<u16> {
        if let Some(&index) = self.index.get(&(source_id, end_col)) {
            return Some(index);
        }
        if self.entries.len() >= capacity {
            return None;
        }
        let index = u16::try_from(self.entries.len()).ok()?;
        self.entries.push((source_id, end_col));
        self.index.insert((source_id, end_col), index);
        Some(index)
    }

    /// Returns the `(source identity, end column)` recorded at `index`.
    fn get(&self, index: u16) -> (u32, u32) {
        self.entries[usize::from(index)]
    }
}

static OVERFLOW_ENDS: std::sync::Mutex<OverflowEnds> = std::sync::Mutex::new(OverflowEnds {
    entries: Vec::new(),
    index: std::collections::BTreeMap::new(),
});

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
        match self.overflow_entry() {
            Some((_, end_col)) => end_col,
            None if self.end_col & PACKED_SOURCE_SPAN == 0 => self.end_col,
            None => self.end_col & PACKED_END_COL_MASK,
        }
    }

    /// Returns the included-file identity, or zero for the root/default source.
    pub fn source_id(self) -> u32 {
        match self.overflow_entry() {
            Some((source_id, _)) => source_id,
            None if self.end_col & PACKED_SOURCE_SPAN == 0 => 0,
            None => (self.end_col >> 16) & SOURCE_ID_MASK,
        }
    }

    /// Returns the side-table entry of a span whose end column overflowed the packed field.
    fn overflow_entry(self) -> Option<(u32, u32)> {
        if self.end_col & PACKED_SOURCE_SPAN == 0 || (self.end_col >> 16) & SOURCE_ID_MASK != 0 {
            return None;
        }
        let index = (self.end_col & PACKED_END_COL_MASK) as u16;
        let table = OVERFLOW_ENDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        Some(table.get(index))
    }

    /// Packs an included-file source identity with its end column.
    ///
    /// A column that fits 16 bits is stored inline. A wider one — a valid line in an included
    /// file can run past column 65535, as the root file's can — goes to `OVERFLOW_ENDS`, keeping
    /// the exact end and identity. Only if that table is full does the end saturate at 65535,
    /// which keeps the compile going at the cost of that span's end position.
    fn pack_end_column(end_col: u32, source_id: u32) -> u32 {
        if source_id == 0 {
            return end_col;
        }
        assert!(source_id <= SOURCE_ID_MASK, "source identity exceeds packed span range");
        if end_col <= PACKED_END_COL_MASK {
            return PACKED_SOURCE_SPAN | (source_id << 16) | end_col;
        }
        let mut table = OVERFLOW_ENDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match table.intern(source_id, end_col, OVERFLOW_CAPACITY) {
            Some(index) => PACKED_SOURCE_SPAN | u32::from(index),
            None => PACKED_SOURCE_SPAN | (source_id << 16) | PACKED_END_COL_MASK,
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

    /// An included-file end past the packed 16-bit field keeps its exact column and identity.
    ///
    /// It used to abort the compile (#1292), then saturated at 65535 — which put a token that
    /// STARTS past that column before its own end, so `has_extent()` was false and source maps
    /// dropped the end (#1306).
    #[test]
    fn an_included_end_past_the_packed_field_stays_exact() {
        let point = Span::new_in_source(2, 70_000, 5);
        assert_eq!(point.source_id(), 5);
        assert_eq!(point.end_column(), 70_000);
        assert!(!point.has_extent());

        let token = Span::with_end_from(point, Span::new_in_source(2, 70_010, 5));
        assert_eq!(token.source_id(), 5);
        assert_eq!(token.end_column(), 70_010);
        assert!(token.has_extent(), "an end past the start is an extent");

        let wide = Span::with_end_from(Span::new_in_source(2, 6, 5), Span::new_in_source(2, 70_008, 5));
        assert_eq!(wide.end_column(), 70_008, "a span straddling column 65535 keeps its end");

        let merged = Span::new_in_source(2, 6, 5).merge(token);
        assert_eq!(merged.end_column(), 70_010);
        assert_eq!(merged.source_id(), 5);
    }

    /// Overflowing ends stay distinct map keys exactly as inline ones do.
    #[test]
    fn overflowing_ends_keep_spans_distinct_and_equal_where_they_should() {
        let a = Span::with_end_from(Span::new_in_source(3, 70_000, 9), Span::new_in_source(3, 70_004, 9));
        let same = Span::with_end_from(Span::new_in_source(3, 70_000, 9), Span::new_in_source(3, 70_004, 9));
        let longer = Span::with_end_from(Span::new_in_source(3, 70_000, 9), Span::new_in_source(3, 70_009, 9));
        let other_file = Span::with_end_from(Span::new_in_source(3, 70_000, 10), Span::new_in_source(3, 70_004, 10));
        assert_eq!(a, same, "equal ends are one table entry, so the spans compare equal");
        assert_ne!(a, longer);
        assert_ne!(a, other_file, "the same coordinates in another included file stay distinct");
        assert_eq!(other_file.source_id(), 10);
        assert_ne!(a, Span::with_end(3, 70_000, 3, 70_004), "an included span is never a root one");
    }

    /// A full side table falls back to saturating the end rather than aborting, and interning
    /// the same pair twice returns the same index.
    #[test]
    fn a_full_overflow_table_degrades_instead_of_aborting() {
        let mut table = OverflowEnds {
            entries: Vec::new(),
            index: std::collections::BTreeMap::new(),
        };
        assert_eq!(table.intern(1, 70_000, 2), Some(0));
        assert_eq!(table.intern(1, 70_001, 2), Some(1));
        assert_eq!(table.intern(1, 70_000, 2), Some(0), "an existing pair is found, not re-added");
        assert_eq!(table.intern(1, 70_002, 2), None, "no room left");
        assert_eq!(table.get(1), (1, 70_001));
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
