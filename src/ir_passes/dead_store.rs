//! Purpose:
//! CFG-aware dead store elimination over PHP local slots. Removes `store_local`
//! instructions whose stored value is never read on any path before the slot is
//! overwritten or the function exits.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Only non-refcounted (`!php_type_needs_lifetime_tracking`) `PhpLocal` slots
//!   that are exclusively accessed through plain `load_local`/`store_local` are
//!   eligible. Assignment lowering wraps refcounted slots with separate
//!   `acquire`/`release` instructions, so dropping a refcounted store in
//!   isolation would unbalance reference counts; scalar slots carry no such
//!   ownership ops and their scope-exit cleanup is a no-op, so removing a dead
//!   scalar store is refcount-neutral and behavior-preserving.
//! - This complements the peephole pass's per-block, value-equality store
//!   forwarding: this pass is liveness-based and crosses block boundaries, so it
//!   removes a store of a *different* value whose result is never observed.
//! - Stores are neutralized to `nop` rather than physically removed, keeping the
//!   instruction/value tables stable for the validator and later passes.
//! - Dynamic eval observes the PHP frame without explicit slot operands, so its
//!   functions keep all scalar stores.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BasicBlock, BlockId, DataPool, Effects, Function, Immediate, InstId, LocalKind,
    LocalSlotId, Op, Ownership, Terminator, Value,
};

use super::cfg::successors;
use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Liveness-based dead store elimination over scalar PHP local slots.
pub struct DeadStore;

impl IrPass for DeadStore {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "dead-store"
    }

    /// Neutralizes dead scalar `store_local` instructions and reports whether any
    /// store changed. The literal pool is unused because the pass never
    /// materializes new constants.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let eligible = eligible_slots(function, data);
        if eligible.is_empty() {
            return false;
        }
        let dead = collect_dead_stores(function, &eligible);
        if dead.is_empty() {
            return false;
        }
        for inst_id in dead {
            if let Some(inst) = function.instruction_mut(inst_id) {
                neutralize_to_nop(inst);
            }
        }
        true
    }
}

/// Returns the set of local slots that are safe to reason about for dead store
/// elimination.
///
/// A slot qualifies when it is an ordinary `PhpLocal`, its storage type needs no
/// lifetime tracking (so no `acquire`/`release` ownership ops surround its
/// stores), and every instruction naming the slot is a plain `load_local` or
/// `store_local`. Any other slot-naming op (ref-cell promote/alias/release,
/// `unset_local`, `zero_local_slot`, static-local or global access, list unpack, …)
/// makes the slot ineligible because it could read or alias the slot in a way this
/// pass does not model.
fn eligible_slots(function: &Function, data: &DataPool) -> HashSet<LocalSlotId> {
    if super::local_scope::has_dynamic_eval(function) {
        return HashSet::new();
    }
    let mut eligible: HashSet<LocalSlotId> = function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter(|local| !Ownership::php_type_needs_lifetime_tracking(&local.php_type))
        .map(|local| local.id)
        .collect();
    if eligible.is_empty() {
        return eligible;
    }

    for inst in &function.instructions {
        if matches!(inst.op, Op::LoadLocal | Op::StoreLocal) {
            continue;
        }
        for slot in immediate_slots(inst.immediate.as_ref()) {
            eligible.remove(&slot);
        }
    }
    if eligible.is_empty() {
        return eligible;
    }

    exclude_address_escaping_slots(function, &mut eligible);
    exclude_eval_literal_read_slots(function, data, &mut eligible);
    eligible
}

/// Drops slots read implicitly by literal `eval` calls from dead-store eligibility.
///
/// Direct eval AOT read-param lowering materializes those slot loads in codegen
/// rather than as explicit EIR `LoadLocal` instructions, so the liveness scan
/// cannot see them. Excluding the known read names keeps stores that feed the
/// eval fragment alive while leaving ordinary scalar locals eligible.
fn exclude_eval_literal_read_slots(
    function: &Function,
    data: &DataPool,
    eligible: &mut HashSet<LocalSlotId>,
) {
    let slots_by_name: HashMap<String, LocalSlotId> = function
        .locals
        .iter()
        .filter(|local| eligible.contains(&local.id))
        .filter_map(|local| local.name.as_ref().map(|name| (name.clone(), local.id)))
        .collect();
    if slots_by_name.is_empty() {
        return;
    }

    for inst in &function.instructions {
        if inst.op != Op::EvalLiteralCall {
            continue;
        }
        let (data_id, profile) = match inst.immediate {
            Some(Immediate::Data(data_id)) => (data_id, crate::source::EvalSourceProfile::permissive(false)),
            Some(Immediate::ProfiledData {
                data: data_id,
                profile,
            }) => (data_id, profile),
            _ => continue,
        };
        let Some(fragment) = data.strings.get(data_id.as_raw() as usize) else {
            continue;
        };
        let plan = crate::eval_aot::plan_literal_fragment_with_static_calls(
            fragment,
            profile,
            |_, _| false,
        );
        for name in plan.reads() {
            if let Some(slot) = slots_by_name.get(name) {
                eligible.remove(slot);
            }
        }
    }
}

/// Drops slots whose loaded value can be reinterpreted as the slot's address.
///
/// A by-reference call argument or closure capture aliases the underlying slot:
/// codegen resolves the argument value back to its defining `load_local` and
/// passes the slot's address, so the callee can read or mutate the slot through
/// the alias. That makes every store to the slot observable, which this pass's
/// forward `load_local`-only liveness cannot see. The judgement itself lives in
/// `super::by_ref_alias`, shared with `super::immutable_local_loads`, which needs
/// exactly the same exclusion for its own (otherwise unsound) immutability proof.
fn exclude_address_escaping_slots(function: &Function, eligible: &mut HashSet<LocalSlotId>) {
    for slot in super::by_ref_alias::address_escaping_slots(function) {
        eligible.remove(&slot);
    }
}

/// Returns the local slots named by an instruction immediate, covering both the
/// single-slot and slot-pair (ref-cell) immediate shapes.
fn immediate_slots(immediate: Option<&Immediate>) -> Vec<LocalSlotId> {
    match immediate {
        Some(Immediate::LocalSlot(slot)) => vec![*slot],
        Some(Immediate::LocalSlotPair { first, second })
        | Some(Immediate::PropertyRefCell { first, second, .. }) => vec![*first, *second],
        Some(Immediate::IterStart(metadata)) => metadata.local_slots().collect(),
        _ => Vec::new(),
    }
}

/// Maps an eligible load or store to its dense liveness index.
fn instruction_slot(
    function: &Function,
    inst_id: InstId,
    indices: &[Option<usize>],
) -> Option<(Op, usize)> {
    let inst = function.instruction(inst_id)?;
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return None;
    };
    if !matches!(inst.op, Op::LoadLocal | Op::StoreLocal) {
        return None;
    }
    Some((inst.op, indices.get(slot.as_raw() as usize).copied().flatten()?))
}

/// Identifies a possible observation of all eligible locals by an implicit catch edge.
fn observes_handler(function: &Function, inst_id: InstId, handled: bool) -> bool {
    handled && function.instruction(inst_id).is_some_and(|inst| inst.effects.contains(Effects::MAY_THROW))
}

/// Occupancy bits indexed only by eligible local slots.
#[derive(Clone, PartialEq, Eq)]
struct LiveSlots {
    words: Vec<u64>,
}

impl LiveSlots {
    /// Creates the empty liveness set for the given slot count.
    fn empty(count: usize) -> Self {
        Self { words: vec![0; count.div_ceil(u64::BITS as usize)] }
    }

    /// Tests whether a slot is live.
    fn contains(&self, slot: usize) -> bool {
        self.words[slot / 64] & (1 << (slot % 64)) != 0
    }

    /// Marks a slot as live.
    fn insert(&mut self, slot: usize) {
        self.words[slot / 64] |= 1 << (slot % 64);
    }

    /// Kills the previous value of a slot.
    fn remove(&mut self, slot: usize) {
        self.words[slot / 64] &= !(1 << (slot % 64));
    }

    /// Marks every slot from another flow path as live.
    fn include(&mut self, other: &Self) {
        for (word, incoming) in self.words.iter_mut().zip(&other.words) {
            *word |= incoming;
        }
    }
}

/// Summarizes a block's effect on its entry set without rescanning every instruction.
struct BlockLiveness {
    successors: Vec<BlockId>,
    first_reads: Vec<usize>,
    first_writes: Vec<usize>,
    observes_handler: bool,
    throw_exit: bool,
}

impl BlockLiveness {
    /// Retains each slot's first access before the first possible handler observation.
    fn new(function: &Function, block: &BasicBlock, indices: &[Option<usize>], handled: bool,
        seen: &mut [bool]) -> Self {
        let mut first_reads = Vec::new();
        let mut first_writes = Vec::new();
        let throw_exit = handled && matches!(block.terminator.as_ref(), Some(Terminator::Throw { .. }));
        let mut has_handler_observation = throw_exit;
        for &inst_id in &block.instructions {
            if observes_handler(function, inst_id, handled) {
                has_handler_observation = true;
                break;
            }
            let Some((op, slot)) = instruction_slot(function, inst_id, indices) else { continue; };
            if std::mem::replace(&mut seen[slot], true) {
                continue;
            }
            if op == Op::LoadLocal {
                first_reads.push(slot);
            } else {
                first_writes.push(slot);
            }
        }
        for &slot in first_reads.iter().chain(&first_writes) {
            seen[slot] = false;
        }
        Self {
            successors: block.terminator.as_ref().map_or_else(Vec::new, successors),
            first_reads,
            first_writes,
            observes_handler: has_handler_observation,
            throw_exit,
        }
    }

    /// Merges normal successor entry sets, accounting for an implicit throw edge.
    fn live_out(&self, live_in: &[LiveSlots], all: &LiveSlots, live: &mut LiveSlots) {
        if self.throw_exit {
            live.words.copy_from_slice(&all.words);
            return;
        }
        live.words.fill(0);
        for successor in &self.successors {
            if let Some(incoming) = live_in.get(successor.as_raw() as usize) {
                live.include(incoming);
            }
        }
    }

    /// Applies the precomputed block transfer to the current successor state.
    fn live_in(&self, live_in: &[LiveSlots], all: &LiveSlots, live: &mut LiveSlots) {
        if self.observes_handler {
            live.words.copy_from_slice(&all.words);
        } else {
            self.live_out(live_in, all, live);
        }
        for &slot in &self.first_writes {
            live.remove(slot);
        }
        for &slot in &self.first_reads {
            live.insert(slot);
        }
    }
}

/// Solves backward liveness by revisiting predecessors whose successor set changed.
fn compute_slot_live_in(blocks: &[BlockLiveness], all: &LiveSlots) -> Vec<LiveSlots> {
    let mut live = LiveSlots { words: vec![0; all.words.len()] };
    let mut live_in = vec![live.clone(); blocks.len()];
    let mut predecessors = vec![Vec::new(); blocks.len()];
    for (index, block) in blocks.iter().enumerate() {
        if block.observes_handler {
            continue;
        }
        for successor in &block.successors {
            if let Some(incoming) = predecessors.get_mut(successor.as_raw() as usize) {
                incoming.push(index);
            }
        }
    }
    // Seed every block, including dead continuations, in reverse layout order.
    let mut pending: Vec<usize> = (0..blocks.len()).collect();
    let mut queued = vec![true; blocks.len()];
    while let Some(index) = pending.pop() {
        queued[index] = false;
        blocks[index].live_in(&live_in, all, &mut live);
        if live == live_in[index] {
            continue;
        }
        std::mem::swap(&mut live, &mut live_in[index]);
        for &predecessor in &predecessors[index] {
            if !std::mem::replace(&mut queued[predecessor], true) {
                pending.push(predecessor);
            }
        }
    }
    live_in
}

/// Finds scalar stores overwritten or never observed before the function exits.
fn collect_dead_stores(function: &Function, eligible: &HashSet<LocalSlotId>) -> Vec<InstId> {
    let mut indices = vec![None; function.locals.len()];
    let mut all = LiveSlots::empty(eligible.len());
    for (index, slot) in function.locals.iter().map(|local| local.id)
        .filter(|slot| eligible.contains(slot)).enumerate() {
        indices[slot.as_raw() as usize] = Some(index);
        all.insert(index);
    }
    let handled = function.instructions.iter().any(|inst| inst.op == Op::TryPushHandler);
    let mut seen = vec![false; eligible.len()];
    let blocks: Vec<_> = function.blocks.iter().map(|block| {
        BlockLiveness::new(function, block, &indices, handled, &mut seen)
    }).collect();
    let live_in = compute_slot_live_in(&blocks, &all);
    let mut live = LiveSlots::empty(eligible.len());
    let mut dead = Vec::new();
    for (block, flow) in function.blocks.iter().zip(&blocks) {
        flow.live_out(&live_in, &all, &mut live);
        for &inst_id in block.instructions.iter().rev() {
            if observes_handler(function, inst_id, handled) {
                live.words.copy_from_slice(&all.words);
                continue;
            }
            match instruction_slot(function, inst_id, &indices) {
                Some((Op::StoreLocal, slot)) => {
                    if !live.contains(slot) && store_value_is_non_heap(function, inst_id) {
                        dead.push(inst_id);
                    }
                    live.remove(slot);
                }
                Some((Op::LoadLocal, slot)) => live.insert(slot),
                _ => {}
            }
        }
    }
    dead
}

/// Returns true when a `store_local`'s value operand has non-heap ownership.
///
/// Eligible slots are already restricted to non-lifetime-tracked storage, so the
/// stored value is expected to be a scalar; this guards against an unexpected
/// owning value reaching a scalar slot, which would make dropping the store
/// leak the value.
fn store_value_is_non_heap(function: &Function, inst_id: InstId) -> bool {
    let Some(inst) = function.instruction(inst_id) else {
        return false;
    };
    let Some(&value) = inst.operands.first() else {
        return false;
    };
    function
        .value(value)
        .map(|v: &Value| v.ownership == Ownership::NonHeap)
        .unwrap_or(false)
}
