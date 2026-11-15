//! Purpose:
//! Computes which PHP locals still hold a valid frame value at each EIR
//! instruction of an optimized debug build. The codegen backend attaches the
//! result to every source site so the debugger can report `<optimized out>`
//! instead of reading an uninitialized or stale slot.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_function` when
//!   `Module::debug_optimized` is set.
//!
//! Key details:
//! - This is a forward must-defined analysis: a local is available only when
//!   every path from function entry executed a store for its slot. Joins
//!   intersect their predecessors, and loop headers reach a fixed point the
//!   same way.
//! - Parameters enter defined: codegen stores every incoming argument in its
//!   slot before the first source site.
//! - Functions with implicit exception edges or dynamic eval are treated as
//!   fully available: their slots are not promoted away, and the explicit CFG
//!   does not model the writes those paths perform.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ir::{Function, Immediate, InstId, LocalSlotId, Op};

/// Per-instruction availability of the debug-visible PHP locals.
pub(crate) struct DebugLocalAvailability {
    /// Compact debug index of each tracked local slot.
    index: HashMap<LocalSlotId, u32>,
    /// Availability bitset before each instruction. Absent means fully
    /// available, the conservative fallback for functions this analysis skips.
    by_instruction: HashMap<InstId, Vec<u64>>,
    /// Number of compact debug indices.
    count: u32,
}

impl DebugLocalAvailability {
    /// Returns the compact debug index assigned to `slot`.
    pub(crate) fn index_of(&self, slot: LocalSlotId) -> Option<u32> {
        self.index.get(&slot).copied()
    }

    /// Returns the compact indices that are not available before `instruction`.
    ///
    /// An unknown instruction, or an analysis that deliberately skipped the
    /// function, reports no dead locals rather than claiming a value is gone.
    pub(crate) fn dead_indices(&self, instruction: InstId) -> Vec<u32> {
        let Some(state) = self.by_instruction.get(&instruction) else {
            return Vec::new();
        };
        (0..self.count)
            .filter(|index| {
                let word = state
                    .get((*index / 64) as usize)
                    .copied()
                    .unwrap_or(0);
                word & (1u64 << (index % 64)) == 0
            })
            .collect()
    }
}

/// Computes must-defined availability for the supplied debug-visible slots.
pub(crate) fn compute_debug_availability(
    function: &Function,
    slots: &[LocalSlotId],
) -> DebugLocalAvailability {
    let index: HashMap<LocalSlotId, u32> = slots
        .iter()
        .enumerate()
        .map(|(position, slot)| (*slot, position as u32))
        .collect();
    let empty = DebugLocalAvailability {
        index: index.clone(),
        by_instruction: HashMap::new(),
        count: slots.len() as u32,
    };
    if slots.is_empty()
        || super::cfg::has_exception_handlers(function)
        || super::local_scope::has_dynamic_eval(function)
    {
        return empty;
    }

    let words = slots.len().div_ceil(64);
    let full = vec![u64::MAX; words];
    let zero = vec![0u64; words];
    let mut parameters = zero.clone();
    for raw in 0..function.params.len() as u32 {
        set_bit(&mut parameters, &index, LocalSlotId::from_raw(raw));
    }

    let order = super::cfg::reverse_postorder(function);
    let reachable: HashSet<_> = order.iter().copied().collect();
    let predecessors = super::cfg::predecessors(function);
    let block_count = function.blocks.len();
    let mut incoming = vec![full.clone(); block_count];
    // Optimistic initialization: an unprocessed back edge must not contribute an
    // empty outgoing state, or a loop header would intersect its parameter state
    // with zero on the first pass and never recover.
    let mut outgoing = vec![full.clone(); block_count];
    if let Some(entry) = function.blocks.iter().position(|block| block.id == function.entry) {
        incoming[entry] = parameters.clone();
    }

    // Must-defined dataflow: every join intersects its reachable predecessors.
    // Initialising non-entry blocks to the top element lets the intersection
    // shrink monotonically, including loop back-edges.
    let mut pending: VecDeque<_> = order.iter().copied().collect();
    while let Some(block) = pending.pop_front() {
        let raw = block.as_raw() as usize;
        let new_incoming = if block == function.entry {
            parameters.clone()
        } else {
            let mut state = full.clone();
            let mut any = false;
            for predecessor in &predecessors[raw] {
                if reachable.contains(predecessor) {
                    any = true;
                    intersect(&mut state, &outgoing[predecessor.as_raw() as usize]);
                }
            }
            if any { state } else { zero.clone() }
        };
        if new_incoming != incoming[raw] {
            incoming[raw] = new_incoming;
        }
        let new_outgoing = transfer(function, block, incoming[raw].clone(), &index);
        if new_outgoing != outgoing[raw] {
            outgoing[raw] = new_outgoing;
            for successor in function
                .blocks
                .get(raw)
                .and_then(|block| block.terminator.as_ref())
                .map(super::cfg::successors)
                .unwrap_or_default()
            {
                if reachable.contains(&successor) && !pending.contains(&successor) {
                    pending.push_back(successor);
                }
            }
        }
    }

    let mut by_instruction = HashMap::new();
    for block in &function.blocks {
        let reachable_block = reachable.contains(&block.id);
        let mut state = if reachable_block {
            incoming[block.id.as_raw() as usize].clone()
        } else {
            zero.clone()
        };
        for instruction_id in &block.instructions {
            by_instruction.insert(*instruction_id, state.clone());
            apply_instruction(function, *instruction_id, &mut state, &index);
        }
    }

    DebugLocalAvailability {
        index: empty.index,
        by_instruction,
        count: empty.count,
    }
}

/// Applies one instruction's effects to a block's outgoing availability.
fn transfer(
    function: &Function,
    block: crate::ir::BlockId,
    mut state: Vec<u64>,
    index: &HashMap<LocalSlotId, u32>,
) -> Vec<u64> {
    let Some(block) = function.blocks.get(block.as_raw() as usize) else {
        return state;
    };
    for instruction in &block.instructions {
        apply_instruction(function, *instruction, &mut state, index);
    }
    state
}

/// Applies one instruction's local-definition effect to `state`.
fn apply_instruction(
    function: &Function,
    instruction_id: InstId,
    state: &mut [u64],
    index: &HashMap<LocalSlotId, u32>,
) {
    let Some(instruction) = function.instruction(instruction_id) else {
        return;
    };
    match instruction.op {
        Op::StoreLocal | Op::StoreRefCell => {
            if let Some(Immediate::LocalSlot(slot)) = instruction.immediate {
                set_bit(state, index, slot);
            }
        }
        Op::PromoteLocalRefCell | Op::AliasLocalRefCell => {
            if let Some(Immediate::LocalSlotPair { first, .. }) = instruction.immediate {
                set_bit(state, index, first);
            }
        }
        Op::UnsetLocal | Op::ZeroLocalSlot => {
            if let Some(Immediate::LocalSlot(slot)) = instruction.immediate {
                clear_bit(state, index, slot);
            }
        }
        _ => {}
    }
}

/// Marks one tracked slot available.
fn set_bit(state: &mut [u64], index: &HashMap<LocalSlotId, u32>, slot: LocalSlotId) {
    let Some(index) = index.get(&slot).copied() else {
        return;
    };
    state[(index / 64) as usize] |= 1u64 << (index % 64);
}

/// Marks one tracked slot unavailable.
fn clear_bit(state: &mut [u64], index: &HashMap<LocalSlotId, u32>, slot: LocalSlotId) {
    let Some(index) = index.get(&slot).copied() else {
        return;
    };
    state[(index / 64) as usize] &= !(1u64 << (index % 64));
}

/// Intersects `state` with `other` in place.
fn intersect(state: &mut [u64], other: &[u64]) {
    for (word, other) in state.iter_mut().zip(other) {
        *word &= *other;
    }
}

/// Formats sorted debug indices as compact `0-2,4` ranges for a marker field.
pub(crate) fn format_dead_ranges(indices: &[u32]) -> String {
    let mut ranges: Vec<String> = Vec::new();
    let mut start = None;
    let mut previous = 0u32;
    for &index in indices {
        match start {
            None => {
                start = Some(index);
                previous = index;
            }
            Some(_) if index == previous + 1 => {
                previous = index;
            }
            Some(first) => {
                ranges.push(if first == previous {
                    first.to_string()
                } else {
                    format!("{first}-{previous}")
                });
                start = Some(index);
                previous = index;
            }
        }
    }
    if let Some(first) = start {
        ranges.push(if first == previous {
            first.to_string()
        } else {
            format!("{first}-{previous}")
        });
    }
    ranges.join(",")
}
