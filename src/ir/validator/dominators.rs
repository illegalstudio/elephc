//! Purpose:
//! Retains compact dominator sets for EIR validation queries.
//!
//! Called from:
//! - The validator's existing fixed-point dominator computation and use checks.
//!
//! Key details:
//! - Dense block IDs index the original bitsets without expanding them into hash tables.
//! - Singleton queries preserve the validator's unreachable-block convention.

use crate::ir::BlockId;

/// Computed dominators with constant-time membership and singleton-self queries.
pub(super) struct DominatorSets {
    bits: Vec<Vec<u64>>,
    singleton_self: Vec<bool>,
}

impl DominatorSets {
    /// Keeps the computed bits and caches whether each row contains exactly its own block.
    pub(super) fn new(bits: Vec<Vec<u64>>) -> Self {
        let singleton_self = bits.iter().enumerate().map(|(block, words)| {
            words.iter().enumerate().all(|(index, &word)| {
                let expected = if index == block / 64 { 1_u64 << (block % 64) } else { 0 };
                word == expected
            }) && words.get(block / 64).is_some()
        }).collect();
        Self { bits, singleton_self }
    }

    /// Answers false for either unknown block, matching the previous block-map lookup.
    pub(super) fn dominates(&self, definition: BlockId, used_in: BlockId) -> bool {
        let definition = definition.as_raw() as usize;
        if definition >= self.bits.len() { return false; }
        self.bits.get(used_in.as_raw() as usize)
            .and_then(|words| words.get(definition / 64))
            .is_some_and(|word| word & (1_u64 << (definition % 64)) != 0)
    }

    /// Preserves the exact singleton-self rule, including the separately handled entry block.
    pub(super) fn is_singleton(&self, block: BlockId) -> bool {
        self.singleton_self.get(block.as_raw() as usize).copied().unwrap_or(false)
    }
}
