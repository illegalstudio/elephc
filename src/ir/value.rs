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
        if matches!(ty, PhpType::Packed(_)) {
            return Ownership::Borrowed;
        }
        if Self::php_type_needs_lifetime_tracking(ty) {
            Ownership::MaybeOwned
        } else {
            Ownership::NonHeap
        }
    }

    /// Returns true when the PHP type can carry cleanup or retain responsibility.
    ///
    /// DELIBERATELY WIDER THAN [`PhpType::is_refcounted`]: `Str`, `Callable`, `Buffer`,
    /// and `Resource` are lifetime-tracked in the IR layer while `is_refcounted` — which
    /// the BACKEND's `emit_incref_if_refcounted` consults — does not list them. That
    /// divergence is load bearing, not an oversight: `LoweringContext::store_local`
    /// relies on it to retain a string stored into a `static` local, because the
    /// backend's incref is a silent no-op for strings. Adding `Str` to `is_refcounted`
    /// to "fix the inconsistency" would make that store retain TWICE and leak one buffer
    /// per assignment. Resources likewise use the dedicated runtime registry retain and
    /// release helpers rather than the heap-block refcount helpers.
    ///
    /// `lifetime_tracking_is_wider_than_refcounted` pins the relationship so the next
    /// person to notice the asymmetry sees why it exists before changing either predicate.
    pub fn php_type_needs_lifetime_tracking(ty: &PhpType) -> bool {
        if matches!(ty, PhpType::Resource(_)) {
            return true;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the deliberate asymmetry between the IR-layer lifetime predicate and the
    /// backend's refcount predicate.
    ///
    /// `Str` MUST be lifetime-tracked (so `store_local` retains a string stored into a
    /// `static` local) and MUST NOT be `is_refcounted` (so the backend's
    /// `emit_incref_if_refcounted` does not retain it a second time). Flipping either half
    /// reintroduces one of two bugs: the static-local string use-after-free, or a leak of
    /// one buffer per static assignment. Every refcounted type must also be
    /// lifetime-tracked — the IR predicate is a strict superset.
    #[test]
    fn lifetime_tracking_is_wider_than_refcounted() {
        assert!(
            Ownership::php_type_needs_lifetime_tracking(&PhpType::Str),
            "Str must stay lifetime-tracked: store_local's static-local retain depends on it"
        );
        assert!(
            !PhpType::Str.is_refcounted(),
            "Str must stay OUT of is_refcounted: the backend would then retain a second time \
             and every `static $s = ''; $s = f();` would leak one buffer per call"
        );
        let stream_resource = PhpType::Resource(Some("stream".to_string()));
        assert!(
            Ownership::php_type_needs_lifetime_tracking(&stream_resource),
            "Resource must be lifetime-tracked so Acquire/Release can retain the runtime registry handle"
        );
        assert_eq!(
            Ownership::for_php_type(&stream_resource),
            Ownership::MaybeOwned,
            "Resource SSA values must carry release-capable default ownership"
        );
        assert!(
            !stream_resource.is_refcounted(),
            "Resource must stay OUT of is_refcounted: it uses registry retain/release rather than heap incref/decref"
        );
        for ty in [
            PhpType::Mixed,
            PhpType::Array(Box::new(PhpType::Int)),
            PhpType::Object("stdClass".to_string()),
            PhpType::Iterable,
        ] {
            assert!(
                ty.is_refcounted(),
                "{ty:?} is expected to be backend-refcounted"
            );
            assert!(
                Ownership::php_type_needs_lifetime_tracking(&ty),
                "{ty:?} is refcounted, so it must also be lifetime-tracked \
                 (the IR predicate is a strict superset)"
            );
        }
    }
}
