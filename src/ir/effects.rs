//! Purpose:
//! Defines the immutable effect bitset used by IR instructions and passes.
//!
//! Called from:
//! - `crate::ir::instr`, `crate::ir::builder`, and future IR optimization passes.
//!
//! Key details:
//! - Effects are conservative PHP-observable summaries. Pure operations have no
//!   bits set and may be removed when their results are unused.

use bitflags::bitflags;

bitflags! {
    /// Conservative effect summary for an EIR operation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Effects: u32 {
        const READS_LOCAL    = 1 << 0;
        const WRITES_LOCAL   = 1 << 1;
        const READS_HEAP     = 1 << 2;
        const WRITES_HEAP    = 1 << 3;
        const READS_GLOBAL   = 1 << 4;
        const WRITES_GLOBAL  = 1 << 5;
        const READS_FS       = 1 << 6;
        const WRITES_FS      = 1 << 7;
        const READS_PROCESS  = 1 << 8;
        const WRITES_PROCESS = 1 << 9;
        const OUTPUT         = 1 << 10;
        const ALLOC_HEAP     = 1 << 11;
        const ALLOC_CONCAT   = 1 << 12;
        const REFCOUNT_OP    = 1 << 13;
        const MAY_THROW      = 1 << 14;
        const MAY_FATAL      = 1 << 15;
        const MAY_WARN       = 1 << 16;
        const MAY_DEOPT      = 1 << 17;
        const BLOCKING_IO    = 1 << 18;
        const NETWORK_IO     = 1 << 19;
    }
}

impl Effects {
    pub const PURE: Effects = Effects::empty();

    /// Returns true when no effect bits are set.
    pub fn is_pure(self) -> bool {
        self.is_empty()
    }

    /// Returns true when the operation can read program-visible state.
    pub fn may_observe(self) -> bool {
        self.intersects(
            Effects::READS_LOCAL
                | Effects::READS_HEAP
                | Effects::READS_GLOBAL
                | Effects::READS_FS
                | Effects::READS_PROCESS
                | Effects::NETWORK_IO,
        )
    }

    /// Returns true when the operation can mutate program-visible state.
    pub fn may_mutate(self) -> bool {
        self.intersects(
            Effects::WRITES_LOCAL
                | Effects::WRITES_HEAP
                | Effects::WRITES_GLOBAL
                | Effects::WRITES_FS
                | Effects::WRITES_PROCESS
                | Effects::REFCOUNT_OP,
        )
    }

    /// Returns true when the operation is observable even if its result is unused.
    pub fn is_observable(self) -> bool {
        self.may_mutate()
            || self.intersects(
                Effects::OUTPUT
                    | Effects::ALLOC_HEAP
                    | Effects::ALLOC_CONCAT
                    | Effects::MAY_THROW
                    | Effects::MAY_FATAL
                    | Effects::MAY_WARN
                    | Effects::MAY_DEOPT
                    | Effects::BLOCKING_IO
                    | Effects::NETWORK_IO,
            )
    }

    /// Returns deterministic textual names for all set effect bits.
    pub fn names(self) -> Vec<&'static str> {
        let ordered = [
            (Effects::READS_LOCAL, "reads_local"),
            (Effects::WRITES_LOCAL, "writes_local"),
            (Effects::READS_HEAP, "reads_heap"),
            (Effects::WRITES_HEAP, "writes_heap"),
            (Effects::READS_GLOBAL, "reads_global"),
            (Effects::WRITES_GLOBAL, "writes_global"),
            (Effects::READS_FS, "reads_fs"),
            (Effects::WRITES_FS, "writes_fs"),
            (Effects::READS_PROCESS, "reads_process"),
            (Effects::WRITES_PROCESS, "writes_process"),
            (Effects::OUTPUT, "output"),
            (Effects::ALLOC_HEAP, "alloc_heap"),
            (Effects::ALLOC_CONCAT, "alloc_concat"),
            (Effects::REFCOUNT_OP, "refcount_op"),
            (Effects::MAY_THROW, "may_throw"),
            (Effects::MAY_FATAL, "may_fatal"),
            (Effects::MAY_WARN, "may_warn"),
            (Effects::MAY_DEOPT, "may_deopt"),
            (Effects::BLOCKING_IO, "blocking_io"),
            (Effects::NETWORK_IO, "network_io"),
        ];
        ordered
            .iter()
            .filter_map(|(flag, name)| self.contains(*flag).then_some(*name))
            .collect()
    }
}
