//! Purpose:
//! Retains compact dominator sets for EIR validation queries.
//!
//! Called from:
//! - The validator's existing fixed-point dominator computation and use checks.
//!
//! Key details:
//! - Dominance is an ancestor query on the dominator tree (Euler-tour numbers), so memory is linear
//!   in the block count instead of quadratic.
//! - Singleton queries preserve the validator's unreachable-block convention.

use crate::ir::BlockId;

/// Marks a block outside the dominator tree (unreachable from the entry).
const UNREACHED: u32 = u32::MAX;

/// Computed dominators answered from the dominator tree in constant time.
///
/// The tree is stored as Euler-tour entry/exit numbers (one `u32` pair per block), so memory is
/// linear in the block count. The former per-block bitsets were `blocks * blocks / 8` bytes: a
/// body of 200k blocks needed 5 GB for its dominator rows alone.
pub(super) struct DominatorSets {
    enter: Vec<u32>,
    exit: Vec<u32>,
    entry: usize,
    /// Fixed-point sweeps, retained only for deterministic scheduling regressions.
    #[cfg(test)]
    pub(super) sweeps: usize,
}

impl DominatorSets {
    /// Builds the queries from immediate dominators: `idom[b]` is the parent of reachable block
    /// `b` in the dominator tree (`idom[entry] == entry`), and `u32::MAX` for an unreachable one.
    pub(super) fn from_idoms(idom: &[u32], entry: usize) -> Self {
        let count = idom.len();
        // Children in compressed-row form, so no per-block `Vec` is allocated.
        let mut first_child = vec![0u32; count + 1];
        for (block, &parent) in idom.iter().enumerate() {
            if parent != UNREACHED && block != entry {
                first_child[parent as usize + 1] += 1;
            }
        }
        for index in 0..count {
            first_child[index + 1] += first_child[index];
        }
        let mut fill = first_child.clone();
        let mut children = vec![0u32; first_child[count] as usize];
        for (block, &parent) in idom.iter().enumerate() {
            if parent != UNREACHED && block != entry {
                children[fill[parent as usize] as usize] = block as u32;
                fill[parent as usize] += 1;
            }
        }
        let mut enter = vec![UNREACHED; count];
        let mut exit = vec![UNREACHED; count];
        let mut clock = 0u32;
        if entry < count && idom[entry] != UNREACHED {
            let mut stack = vec![(entry as u32, first_child[entry])];
            enter[entry] = clock;
            clock += 1;
            while let Some((block, next)) = stack.last_mut() {
                let block = *block as usize;
                if *next < first_child[block + 1] {
                    let child = children[*next as usize];
                    *next += 1;
                    enter[child as usize] = clock;
                    clock += 1;
                    stack.push((child, first_child[child as usize]));
                } else {
                    exit[block] = clock;
                    clock += 1;
                    stack.pop();
                }
            }
        }
        Self {
            enter,
            exit,
            entry,
            #[cfg(test)]
            sweeps: 0,
        }
    }

    /// Answers false for either unknown block; an unreachable block is dominated only by itself.
    pub(super) fn dominates(&self, definition: BlockId, used_in: BlockId) -> bool {
        let definition = definition.as_raw() as usize;
        let used_in = used_in.as_raw() as usize;
        if definition >= self.enter.len() || used_in >= self.enter.len() {
            return false;
        }
        if self.enter[used_in] == UNREACHED {
            return definition == used_in;
        }
        if self.enter[definition] == UNREACHED {
            return false;
        }
        self.enter[definition] <= self.enter[used_in] && self.exit[used_in] <= self.exit[definition]
    }

    /// Whether the block's dominator set is exactly itself: the entry and every unreachable block.
    pub(super) fn is_singleton(&self, block: BlockId) -> bool {
        let block = block.as_raw() as usize;
        block < self.enter.len() && (block == self.entry || self.enter[block] == UNREACHED)
    }

    /// Number of blocks the tree covers (for the linear-size regression).
    #[cfg(test)]
    pub(super) fn tree_words(&self) -> usize {
        self.enter.len() + self.exit.len()
    }

    /// Records how many fixed-point sweeps computed these sets.
    #[cfg(test)]
    pub(super) fn with_sweeps(mut self, sweeps: usize) -> Self {
        self.sweeps = sweeps;
        self
    }

    /// Materializes the block-id map this analysis used to return.
    ///
    /// Test-only: it exists so the bitset accessor can be differentially compared against the
    /// shape it replaced, on the same function, for every (block, candidate) pair.
    #[cfg(test)]
    pub(super) fn to_block_id_map(
        &self,
        function: &crate::ir::function::Function,
    ) -> crate::fast_hash::FastMap<BlockId, crate::fast_hash::FastSet<BlockId>> {
        let mut map = crate::fast_hash::FastMap::default();
        for block in &function.blocks {
            let mut set = crate::fast_hash::FastSet::default();
            for candidate in &function.blocks {
                if self.dominates(candidate.id, block.id) {
                    set.insert(candidate.id);
                }
            }
            map.insert(block.id, set);
        }
        map
    }
}
