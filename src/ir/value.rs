//! Purpose:
//! Defines SSA values, value identifiers, definition sites, and ownership state.
//!
//! Called from:
//! - `crate::ir::function`, `crate::ir::builder`, and `crate::ir::validator`.
//!
//! Key details:
//! - Each `ValueId` indexes exactly one entry in a function-local value table.
//!   Ownership tracks cleanup responsibility at SSA-value granularity.

use crate::ir::block::BlockId;
use crate::ir::instr::InstId;
use crate::ir::types::IrType;
use crate::types::PhpType;

/// Function-local identifier for an SSA value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueId(u32);

impl ValueId {
    /// Creates a value identifier from its raw zero-based table index.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw zero-based table index represented by this identifier.
    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// Function-local SSA value metadata.
#[derive(Debug, Clone)]
pub struct Value {
    pub ir_type: IrType,
    pub php_type: PhpType,
    pub def: ValueDef,
    pub ownership: Ownership,
}

/// Definition site for a function-local SSA value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDef {
    BlockParam { block: BlockId, index: u16 },
    Instruction {
        block: BlockId,
        index: u32,
        inst: InstId,
    },
}

/// Ownership state attached to each SSA value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ownership {
    NonHeap,
    Owned,
    Borrowed,
    MaybeOwned,
    Persistent,
    Moved,
}

impl Ownership {
    /// Returns the default ownership state for a value produced from a PHP type.
    pub fn for_php_type(ty: &PhpType) -> Self {
        let ty = ty.codegen_repr();
        if matches!(ty, PhpType::Packed(_)) {
            return Ownership::Borrowed;
        }
        if Self::php_type_needs_lifetime_tracking(&ty) {
            Ownership::MaybeOwned
        } else {
            Ownership::NonHeap
        }
    }

    /// Returns true when the PHP type can carry cleanup or retain responsibility.
    pub fn php_type_needs_lifetime_tracking(ty: &PhpType) -> bool {
        let ty = ty.codegen_repr();
        matches!(ty, PhpType::Str | PhpType::Callable | PhpType::Buffer(_)) || ty.is_refcounted()
    }

    /// Returns whether a release operation may decrement this value's runtime ownership.
    pub(crate) fn may_require_release(self) -> bool {
        matches!(self, Ownership::Owned | Ownership::MaybeOwned)
    }

    /// Merges two ownership states at a CFG join.
    pub fn merge(self, other: Self) -> Self {
        use Ownership::*;
        match (self, other) {
            (NonHeap, NonHeap) => NonHeap,
            (Owned, Owned) => Owned,
            (Borrowed, Borrowed) => Borrowed,
            (MaybeOwned, MaybeOwned) => MaybeOwned,
            (Persistent, Persistent) => Persistent,
            (Moved, Moved) => Moved,
            (Moved, _) | (_, Moved) => Moved,
            (NonHeap, x) | (x, NonHeap) => x,
            (MaybeOwned, _) | (_, MaybeOwned) => MaybeOwned,
            (Owned, Borrowed) | (Borrowed, Owned) => MaybeOwned,
            (Persistent, _) | (_, Persistent) => MaybeOwned,
        }
    }

    /// Formats the ownership state using the EIR textual format spelling.
    pub fn as_eir(self) -> &'static str {
        match self {
            Ownership::NonHeap => "nonheap",
            Ownership::Owned => "owned",
            Ownership::Borrowed => "borrowed",
            Ownership::MaybeOwned => "maybe_owned",
            Ownership::Persistent => "persistent",
            Ownership::Moved => "moved",
        }
    }
}
