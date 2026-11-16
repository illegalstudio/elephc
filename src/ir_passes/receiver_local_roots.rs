//! Purpose:
//! Proves when a stable local object owner makes an extra method receiver lease redundant.
//!
//! Called from:
//! - `borrowed_receiver_roots`, before method bodies are spliced.
//!
//! Key details:
//! - A single owning entry store must dominate every read, with no alias or later slot mutation.
//! - Exception handlers and special local storage fail closed.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op, Ownership, ValueDef};
use crate::types::PhpType;

/// Finds hidden receiver roots backed by an unchanged, definitely initialized local owner.
pub(super) fn owned_local_roots(function: &Function) -> HashSet<LocalSlotId> {
    if function.flags.is_main || super::cfg::has_exception_handlers(function)
        || !super::cfg::predecessors(function)[function.entry.as_raw() as usize].is_empty()
    { return HashSet::new(); }
    let escaping = super::by_ref_alias::address_escaping_slots(function);
    let mut owners: HashSet<_> = function.locals.iter().filter(|local| {
        local.kind == LocalKind::PhpLocal
            && local.id.as_raw() as usize >= function.params.len()
            && matches!(&local.php_type, PhpType::Object(name) if !name.is_empty())
            && !escaping.contains(&local.id)
    }).map(|local| local.id).collect();
    if owners.is_empty() { return HashSet::new(); }
    let mut stores: HashMap<LocalSlotId, Vec<_>> = HashMap::new();
    let mut loads: HashMap<LocalSlotId, Vec<_>> = HashMap::new();
    let mut root_stores: HashMap<LocalSlotId, Vec<_>> = HashMap::new();
    let mut invalid_roots = HashSet::new();
    for block in &function.blocks {
        for (index, &id) in block.instructions.iter().enumerate() {
            let inst = &function.instructions[id.as_raw() as usize];
            match inst.immediate {
                Some(Immediate::LocalSlot(slot)) => {
                    match inst.op {
                        Op::StoreLocal => stores.entry(slot).or_default().push((block.id, index, id)),
                        Op::LoadLocal => loads.entry(slot).or_default().push((block.id, index)),
                        _ => { owners.remove(&slot); }
                    }
                    if function.locals.get(slot.as_raw() as usize)
                        .is_some_and(|local| local.kind == LocalKind::HiddenTemp)
                    {
                        match inst.op {
                            Op::StoreLocal => root_stores.entry(slot).or_default().push(id),
                            Op::PushCallOperandOwner | Op::PopCallOperandOwner | Op::ReleaseLocalSlot => {}
                            _ => { invalid_roots.insert(slot); }
                        }
                    }
                }
                Some(Immediate::LocalSlotPair { first, second }) => {
                    owners.remove(&first);
                    owners.remove(&second);
                    invalid_roots.extend([first, second]);
                }
                Some(Immediate::IterStart(ref metadata)) => {
                    for slot in metadata.local_slots() {
                        owners.remove(&slot);
                        invalid_roots.insert(slot);
                    }
                }
                _ => {}
            }
        }
    }
    let dominance = super::dominance::compute_dominance(function);
    owners.retain(|slot| {
        let Some(writes) = stores.get(slot) else { return false; };
        let [(block, index, id)] = writes.as_slice() else { return false; };
        if *block != function.entry { return false; }
        let inst = &function.instructions[id.as_raw() as usize];
        if !inst.operands.first().and_then(|&value| function.value(value)).is_some_and(|value| {
            value.ownership == Ownership::Owned
                && value.php_type == function.locals[slot.as_raw() as usize].php_type
        }) { return false; }
        loads.get(slot).is_some_and(|reads| reads.iter().all(|&(read_block, read_index)| {
            if read_block == *block { read_index > *index } else { dominance.dominates(*block, read_block) }
        }))
    });
    root_stores.into_iter().filter_map(|(slot, stores)| {
        if invalid_roots.contains(&slot) || stores.len() != 1 { return None; }
        let store = &function.instructions[stores[0].as_raw() as usize];
        let ValueDef::Instruction { inst, .. } = function.value(*store.operands.first()?)?.def else { return None; };
        let acquire = function.instruction(inst)?;
        if acquire.op != Op::Acquire { return None; }
        let ValueDef::Instruction { inst, .. } = function.value(*acquire.operands.first()?)?.def else { return None; };
        let load = function.instruction(inst)?;
        let Some(Immediate::LocalSlot(owner)) = load.immediate else { return None; };
        (load.op == Op::LoadLocal && owners.contains(&owner)).then_some(slot)
    }).collect()
}
