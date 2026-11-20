//! Purpose:
//! Forwards the record address held by a packed element-view local (`$p = $points[$i]`) to
//! the reads of that local in the same block of a buffer-preserving loop.
//!
//! Called from:
//! - `super::hoist_loops`, for every loop that passed the buffer-preserving allowlist.
//!
//! Key details:
//! - A packed record local stores one raw address word, neither retained nor released, so a
//!   read of the slot returns exactly the value last stored into it. Forwarding that value
//!   makes the view loop read fields through the address `buffer_get_resolved` computed, the
//!   same instructions the direct `$points[$i]->x` spelling already produced.
//! - A slot whose address can escape is skipped: a by-reference alias could rewrite it.
//! - Forwarding never crosses a block boundary or an instruction that names the slot as
//!   storage, and the store itself is kept, so a read in another block still sees the slot.
//! - Only loops on the allowlist are rewritten. Their bodies run no call that could clobber
//!   a register, so the forwarded address stays in a register for the whole iteration.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Immediate, InstId, LocalKind, LocalSlotId, Op, ValueId};
use crate::types::PhpType;

use super::super::loops::NaturalLoop;
use super::super::rewrite::{neutralize_to_nop, replace_all_uses, resolve_chains};
use super::slots_named_as_storage;

/// Replaces reads of packed element-view locals in one loop and reports whether IR changed.
pub(super) fn forward_view_loads(
    function: &mut Function,
    loop_ref: &NaturalLoop,
    escaping: &HashSet<LocalSlotId>,
) -> bool {
    let mut replacements: HashMap<ValueId, ValueId> = HashMap::new();
    let mut forwarded: Vec<InstId> = Vec::new();
    for &block in &loop_ref.blocks {
        let mut resident: HashMap<LocalSlotId, ValueId> = HashMap::new();
        for &inst_id in &function.blocks[block.as_raw() as usize].instructions {
            let inst = &function.instructions[inst_id.as_raw() as usize];
            match (inst.op, &inst.immediate) {
                (Op::StoreLocal, Some(Immediate::LocalSlot(slot))) => {
                    let stored = inst.operands.first().copied().filter(|&value| {
                        view_slot(function, *slot, escaping) && packed_value(function, value)
                    });
                    match stored {
                        Some(value) => resident.insert(*slot, value),
                        None => resident.remove(slot),
                    };
                }
                (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) => {
                    let (Some(&value), Some(result)) = (resident.get(slot), inst.result) else {
                        continue;
                    };
                    if same_representation(function, value, result) {
                        replacements.insert(result, value);
                        forwarded.push(inst_id);
                    }
                }
                _ => {
                    for slot in slots_named_as_storage(std::iter::once(inst)) {
                        resident.remove(&slot);
                    }
                }
            }
        }
    }
    if replacements.is_empty() {
        return false;
    }
    replace_all_uses(function, &resolve_chains(&replacements));
    for inst_id in forwarded {
        neutralize_to_nop(&mut function.instructions[inst_id.as_raw() as usize]);
    }
    true
}

/// Returns whether `slot` is a non-escaping PHP local that holds a packed record address.
fn view_slot(function: &Function, slot: LocalSlotId, escaping: &HashSet<LocalSlotId>) -> bool {
    !escaping.contains(&slot)
        && (slot.as_raw() as usize) >= function.params.len()
        && function.locals.get(slot.as_raw() as usize).is_some_and(|local| {
            local.kind == LocalKind::PhpLocal
                && matches!(local.php_type.codegen_repr(), PhpType::Packed(_))
        })
}

/// Returns whether `value` is a packed record address.
fn packed_value(function: &Function, value: ValueId) -> bool {
    function
        .value(value)
        .is_some_and(|value| matches!(value.php_type.codegen_repr(), PhpType::Packed(_)))
}

/// Requires the stored address and the load result to share one storage and PHP type.
fn same_representation(function: &Function, stored: ValueId, loaded: ValueId) -> bool {
    let (Some(stored), Some(loaded)) = (function.value(stored), function.value(loaded)) else {
        return false;
    };
    stored.ir_type == loaded.ir_type && stored.php_type == loaded.php_type
}
