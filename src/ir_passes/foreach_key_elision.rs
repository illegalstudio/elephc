//! Purpose:
//! Removes the per-iteration key binding of an array or hash `foreach` whose key local is never
//! read anywhere in the function.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`, both the native and the WASM set.
//!
//! Key details:
//! - Lowering always binds `foreach ($a as $k => $v)` by reading the key (a Mixed cell: one
//!   allocation on the WASM and native hash iterators alike), retaining it, releasing the slot's
//!   previous occupant, storing it, and dropping the read's own reference. When nothing ever
//!   loads the slot, the read, the retain, the store and the drop are unobservable, so they are
//!   neutralized to `nop`. The release of the previous occupant is kept: it may run a destructor
//!   (`$k = new D;` before the loop), and once it ran the slot is zero, so later iterations
//!   release nothing.
//! - The proof is entirely about the slot: every instruction naming it must be a plain
//!   `store_local` or `release_local_slot`. A `load_local` (including `compact`, by-value closure
//!   captures and `get_defined_vars`, which lower to loads), a ref-cell promotion, a by-reference
//!   capture, an `unset`, a global alias or any other slot-naming op keeps the slot. Functions
//!   with dynamic eval, and slots a literal eval fragment reads by name, are left alone because
//!   those reads name no slot operand.
//! - Only keys read from a statically known array (`Heap(Array)`) or hash (`Heap(Hash)`) source
//!   are removed. An object iterator's `key()` is user code and a dynamic iterable may be one.
//! - Every release of the slot stays, so whatever it held before the loop dies at the first
//!   binding as in php, and the epilogue still releases whatever the slot holds.
//! - A parameter slot is never touched: its entry value is the caller's, not a binding this
//!   function made.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrHeapKind, IrType, LocalKind, LocalSlotId, Op,
    ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{count_value_uses, defining_instruction, neutralize_to_nop};

/// Dead foreach key binding elimination.
pub struct ForeachKeyElision;

impl IrPass for ForeachKeyElision {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "foreach-key-elision"
    }

    /// Only functions that read a key at all can contain a candidate.
    fn is_applicable(&self, function: &Function) -> bool {
        function
            .instructions
            .iter()
            .any(|inst| inst.op == Op::IterCurrentKey)
    }

    /// Neutralizes every key binding group whose slot is never loaded and reports whether any
    /// group was removed.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if super::local_scope::has_dynamic_eval(function) {
            return false;
        }
        let groups = collect_key_groups(function);
        if groups.is_empty() {
            return false;
        }
        let mut blocked = slots_with_other_references(function);
        for slot in eval_read_slots(function, data) {
            blocked.insert(slot);
        }
        let mut changed = false;
        for group in groups {
            if blocked.contains(&group.slot) || !slot_can_lose_its_key(function, group.slot) {
                continue;
            }
            for inst_id in group.insts {
                if let Some(inst) = function.instruction_mut(inst_id) {
                    neutralize_to_nop(inst);
                    changed = true;
                }
            }
        }
        changed
    }
}

/// One `iter_current_key` binding: the five instructions lowering emits for it.
struct KeyGroup {
    slot: LocalSlotId,
    insts: Vec<InstId>,
}

/// Finds every `iter_current_key; acquire; release_local_slot; store_local; release` group whose
/// key comes from a statically known array or hash.
fn collect_key_groups(function: &Function) -> Vec<KeyGroup> {
    let uses = count_value_uses(function);
    let mut groups = Vec::new();
    for block in &function.blocks {
        let live: Vec<InstId> = block
            .instructions
            .iter()
            .copied()
            .filter(|&id| function.instruction(id).is_some_and(|inst| inst.op != Op::Nop))
            .collect();
        for position in 2..live.len().saturating_sub(1) {
            if let Some(group) = match_key_group(
                function,
                &uses,
                [
                    live[position - 2],
                    live[position - 1],
                    live[position],
                    live[position + 1],
                ],
            ) {
                groups.push(group);
            }
        }
    }
    groups
}

/// Matches `acquire`, `release_local_slot`, `store_local`, `release` and the key read feeding them.
fn match_key_group(
    function: &Function,
    uses: &HashMap<ValueId, usize>,
    window: [InstId; 4],
) -> Option<KeyGroup> {
    let [acquire_id, release_slot_id, store_id, release_id] = window;
    let acquire = function.instruction(acquire_id)?;
    let release_slot = function.instruction(release_slot_id)?;
    let store = function.instruction(store_id)?;
    let release = function.instruction(release_id)?;
    if acquire.op != Op::Acquire
        || release_slot.op != Op::ReleaseLocalSlot
        || store.op != Op::StoreLocal
        || release.op != Op::Release
    {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = store.immediate else {
        return None;
    };
    if release_slot.immediate != Some(Immediate::LocalSlot(slot)) {
        return None;
    }
    let key = *release.operands.first()?;
    let retained = acquire.result?;
    if acquire.operands.first() != Some(&key) || store.operands.first() != Some(&retained) {
        return None;
    }
    // The key read and the retained copy must each be used exactly where this group uses them.
    if uses.get(&key).copied() != Some(2) || uses.get(&retained).copied() != Some(1) {
        return None;
    }
    let ValueDef::Instruction { inst: key_id, .. } = function.value(key)?.def else {
        return None;
    };
    let key_read = function.instruction(key_id)?;
    if key_read.op != Op::IterCurrentKey || !iterator_source_is_array_or_hash(function, key_read) {
        return None;
    }
    // The slot release stays: it retires what the slot held before the loop, which may be an
    // object whose destructor php runs at the first binding (`$k = new D; foreach (... as $k
    // => ...)`). After it the slot is zero, so repeating it on later iterations releases nothing.
    Some(KeyGroup {
        slot,
        insts: vec![key_id, acquire_id, store_id, release_id],
    })
}

/// Whether the iterator read by `key_read` walks a statically known array or hash.
fn iterator_source_is_array_or_hash(function: &Function, key_read: &crate::ir::Instruction) -> bool {
    let Some(&iterator) = key_read.operands.first() else {
        return false;
    };
    let Some(start) = defining_instruction(function, iterator) else {
        return false;
    };
    if start.op != Op::IterStart {
        return false;
    }
    let Some(&source) = start.operands.first() else {
        return false;
    };
    matches!(
        function.value(source).map(|value| value.ir_type),
        Some(IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash))
    )
}

/// Whether the slot is an ordinary Mixed PHP local that this function (not its caller) bound.
fn slot_can_lose_its_key(function: &Function, slot: LocalSlotId) -> bool {
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    if local.kind != LocalKind::PhpLocal || local.php_type.codegen_repr() != PhpType::Mixed {
        return false;
    }
    !function
        .params
        .iter()
        .any(|param| local.name.as_deref() == Some(param.name.as_str()))
}

/// Returns every slot named by an instruction other than a plain store or slot release.
fn slots_with_other_references(function: &Function) -> HashSet<LocalSlotId> {
    let mut blocked = HashSet::new();
    for inst in &function.instructions {
        if matches!(inst.op, Op::StoreLocal | Op::ReleaseLocalSlot | Op::Nop) {
            continue;
        }
        match inst.immediate.as_ref() {
            Some(Immediate::LocalSlot(slot)) => {
                blocked.insert(*slot);
            }
            Some(Immediate::LocalSlotPair { first, second }) => {
                blocked.insert(*first);
                blocked.insert(*second);
            }
            Some(Immediate::IterStart(metadata)) => blocked.extend(metadata.local_slots()),
            _ => {}
        }
    }
    blocked
}

/// Slots a literal `eval` fragment reads by name without naming them as operands.
fn eval_read_slots(function: &Function, data: &DataPool) -> Vec<LocalSlotId> {
    super::dead_store::eval_literal_read_slots(function, data)
}
