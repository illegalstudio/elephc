//! Purpose:
//! Proves that explicitly rooted temporary slots never defer cleanup across an inline return.
//!
//! Called from:
//! - Callee eligibility, method instruction screening and the caller-loop store guard.
//!
//! Key details:
//! - A slot's complete lifetime must be within one block and one balanced owner-stack scope.
//! - Stores preceding publication and clears following unpublication must be adjacent.

use std::collections::HashSet;

use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op};

/// Returns owning temporary slots whose normal lifetime ends explicitly, including unused slots.
/// Exceptional exits retain the original published owner records and their slot-clearing cleanup.
pub(super) fn scoped_temps(function: &Function) -> HashSet<LocalSlotId> {
    function.locals.iter().filter(|local| matches!(local.kind, LocalKind::OwnedTemp | LocalKind::HiddenTemp))
        .filter(|local| slot_is_scoped(function, local.id))
        .map(|local| local.id).collect()
}

/// Checks a single block-local lifetime without relying on implicit exceptional CFG edges.
fn slot_is_scoped(function: &Function, slot: LocalSlotId) -> bool {
    let mut home = None;
    let mut operations = Vec::new();
    for block in &function.blocks {
        let instructions: Vec<_> = block.instructions.iter()
            .filter_map(|&id| function.instruction(id))
            .filter(|inst| inst.op != Op::Nop).collect();
        for (position, inst) in instructions.iter().enumerate() {
            if inst.immediate != Some(Immediate::LocalSlot(slot)) { continue; }
            if home.is_some_and(|home| home != block.id) { return false; }
            home = Some(block.id);
            if !matches!(inst.op, Op::StoreLocal | Op::PushCallOperandOwner
                | Op::PopCallOperandOwner | Op::UnsetLocal | Op::ReleaseLocalSlot)
            {
                return false;
            }
            if matches!(inst.op, Op::StoreLocal) && operations.is_empty() {
                if !instructions.get(position + 1).is_some_and(|next| {
                    next.op == Op::PushCallOperandOwner
                        && next.immediate == Some(Immediate::LocalSlot(slot))
                }) { return false; }
            }
            if inst.op == Op::PopCallOperandOwner {
                if !instructions.get(position + 1).is_some_and(|next| {
                    matches!(next.op, Op::UnsetLocal | Op::ReleaseLocalSlot)
                        && next.immediate == Some(Immediate::LocalSlot(slot))
                }) { return false; }
            }
            operations.push(inst.op);
        }
    }
    operations.is_empty() || matches!(operations.as_slice(),
        [Op::StoreLocal, Op::PushCallOperandOwner, Op::PopCallOperandOwner, Op::UnsetLocal | Op::ReleaseLocalSlot]
        | [Op::PushCallOperandOwner, Op::StoreLocal, Op::PopCallOperandOwner, Op::UnsetLocal | Op::ReleaseLocalSlot])
}

/// Ensures nested temporary publications still form a real stack within each block.
pub(super) fn owner_scopes_are_balanced(function: &Function) -> bool {
    function.blocks.iter().all(|block| {
        let mut stack = Vec::new();
        for inst in block.instructions.iter().filter_map(|&id| function.instruction(id)) {
            match inst.op {
                Op::PushCallOperandOwner => {
                    let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return false; };
                    stack.push(slot);
                }
                Op::PopCallOperandOwner => {
                    let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return false; };
                    if stack.pop() != Some(slot) { return false; }
                }
                _ => {}
            }
        }
        stack.is_empty()
    })
}

#[cfg(test)]
#[path = "../tests/inline_temporaries_test.rs"]
mod tests;
