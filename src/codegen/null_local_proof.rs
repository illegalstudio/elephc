//! Purpose:
//! Proves, for one `load_local`, that every path reaching it last stored `null` into the slot.
//!
//! Called from:
//! - `crate::codegen::context::FunctionContext::current_load_reads_stored_null()`, on behalf of
//!   `crate::codegen::lower_inst::local_loads::lower_load_local()`.
//!
//! Key details:
//! - A pointer slot (object, array, hash, callable, iterable) keeps its storage when `null` is
//!   stored into it, so the slot then holds a zero pointer that no pointer-typed read may use.
//!   EIR types such a read `null` from the lowering env's flow fact, but that fact is only as good
//!   as the env: a ternary, `match` or `switch` arm does not isolate it, so `$c ? ($o = null) : 0`
//!   leaves `$o` typed `null` after the merge while the other path still holds the object.
//! - The proof therefore reads the EIR itself: within the load's block the most recent
//!   instruction naming the slot must be a `store_local` of a `null` value, or, when no
//!   instruction in the block names it, the same must hold at the end of EVERY predecessor
//!   (a forward must-analysis over the terminator graph, `false` at the entry).
//! - Straight-line trust matches the rest of lowering: within a block a plain frame slot changes
//!   only through instructions that name it. Slots that may hold a reference cell are refused,
//!   since a write through the alias names a different slot.
//! - Exception handlers are entered through implicit edges absent from the terminator graph, so a
//!   function that pushes one only accepts the in-block proof.

use crate::ir::{BlockId, Function, Immediate, InstId, Instruction, LocalKind, LocalSlotId, Op};
use crate::types::PhpType;

use super::local_analysis::terminator_successors;

/// What one instruction sequence leaves in a slot, as far as the proof is concerned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotEffect {
    /// No instruction in the sequence names the slot.
    Transparent,
    /// The last instruction naming the slot stores a `null` value into it.
    StoresNull,
    /// The last instruction naming the slot may leave anything else in it.
    Clobbers,
}

/// Returns whether `slot` holds `null` on every path reaching instruction `load`.
///
/// `slot_may_hold_ref_cell` is the caller's representation fact for the slot; a slot that may be
/// a reference cell on any path is never proven, because writes through its alias do not name it.
pub(super) fn null_store_reaches_load(
    function: &Function,
    load: InstId,
    slot: LocalSlotId,
    slot_may_hold_ref_cell: bool,
) -> bool {
    let is_plain_local = function
        .locals
        .get(slot.as_raw() as usize)
        .is_some_and(|local| local.kind == LocalKind::PhpLocal);
    if !is_plain_local || slot_may_hold_ref_cell {
        return false;
    }
    let Some((block_index, position)) = locate_instruction(function, load) else {
        return false;
    };
    let before_load = &function.blocks[block_index].instructions[..position];
    match slot_effect(function, before_load, slot) {
        SlotEffect::StoresNull => return true,
        SlotEffect::Clobbers => return false,
        SlotEffect::Transparent => {}
    }
    if function.instructions.iter().any(|inst| inst.op == Op::TryPushHandler) {
        return false;
    }
    null_at_block_entries(function, slot)[block_index]
}

/// Returns the block index and in-block position of instruction `id`.
fn locate_instruction(function: &Function, id: InstId) -> Option<(usize, usize)> {
    function.blocks.iter().enumerate().find_map(|(block_index, block)| {
        block
            .instructions
            .iter()
            .position(|inst| *inst == id)
            .map(|position| (block_index, position))
    })
}

/// Folds a straight-line instruction sequence into its last effect on `slot`.
fn slot_effect(function: &Function, instructions: &[InstId], slot: LocalSlotId) -> SlotEffect {
    for inst_id in instructions.iter().rev() {
        let Some(inst) = function.instruction(*inst_id) else {
            return SlotEffect::Clobbers;
        };
        if !names_slot(inst, slot) || inst.op == Op::LoadLocal {
            continue;
        }
        return if inst.op == Op::StoreLocal && stores_null(function, inst) {
            SlotEffect::StoresNull
        } else {
            SlotEffect::Clobbers
        };
    }
    SlotEffect::Transparent
}

/// Returns whether `inst` names `slot` in its immediate.
fn names_slot(inst: &Instruction, slot: LocalSlotId) -> bool {
    match inst.immediate {
        Some(Immediate::LocalSlot(named)) => named == slot,
        Some(Immediate::LocalSlotPair { first, second }) => first == slot || second == slot,
        Some(Immediate::IterStart(metadata)) => metadata.local_slots().any(|named| named == slot),
        _ => false,
    }
}

/// Returns whether a `store_local` stores a value typed `null`.
fn stores_null(function: &Function, store: &Instruction) -> bool {
    store
        .operands
        .first()
        .and_then(|operand| function.value(*operand))
        .is_some_and(|value| value.php_type.codegen_repr() == PhpType::Void)
}

/// Computes, per block, whether `slot` holds `null` on every terminator path into the block.
///
/// A greatest fixpoint: every block but the entry starts optimistic and is lowered until stable.
/// The entry and blocks with no terminator predecessor start `false`, so an uninitialized slot, a
/// parameter, or a block reached some other way is never assumed to hold `null`.
fn null_at_block_entries(function: &Function, slot: LocalSlotId) -> Vec<bool> {
    let block_count = function.blocks.len();
    let mut predecessors: Vec<Vec<BlockId>> = vec![Vec::new(); block_count];
    for block in &function.blocks {
        if let Some(terminator) = &block.terminator {
            for successor in terminator_successors(terminator) {
                if let Some(preds) = predecessors.get_mut(successor.as_raw() as usize) {
                    preds.push(block.id);
                }
            }
        }
    }
    let effects = function
        .blocks
        .iter()
        .map(|block| slot_effect(function, &block.instructions, slot))
        .collect::<Vec<_>>();
    let entry = function.entry.as_raw() as usize;
    let mut null_in = (0..block_count)
        .map(|index| index != entry && !predecessors[index].is_empty())
        .collect::<Vec<_>>();
    let null_out = |null_in: &[bool], index: usize| match effects[index] {
        SlotEffect::StoresNull => true,
        SlotEffect::Clobbers => false,
        SlotEffect::Transparent => null_in[index],
    };
    let mut changed = true;
    while changed {
        changed = false;
        for index in 0..block_count {
            if !null_in[index] {
                continue;
            }
            let all_null = predecessors[index]
                .iter()
                .all(|pred| null_out(&null_in, pred.as_raw() as usize));
            if !all_null {
                null_in[index] = false;
                changed = true;
            }
        }
    }
    null_in
}

#[cfg(test)]
mod tests {
    use super::null_store_reaches_load;
    use crate::codegen::platform::Target;
    use crate::ir::{Function, Immediate, InstId, LocalSlotId, Module, Op};
    use crate::types::PhpType;
    use std::path::Path;

    /// Lowers `source` for the host target.
    fn lower(source: &str) -> Module {
        crate::ir_lower::tests::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::detect_host(),
        )
    }

    /// Returns the named user function of `module`.
    fn function<'m>(module: &'m Module, name: &str) -> &'m Function {
        module
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing function {name}"))
    }

    /// Returns every `load_local` of the PHP local `$name` typed `null`, with its slot.
    fn null_typed_loads(function: &Function, name: &str) -> Vec<(InstId, LocalSlotId)> {
        let slot = function
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{}: missing local ${name}", function.name))
            .id;
        function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().copied())
            .filter(|inst_id| {
                function.instruction(*inst_id).is_some_and(|inst| {
                    inst.op == Op::LoadLocal
                        && inst.immediate == Some(Immediate::LocalSlot(slot))
                        && inst.result_php_type.codegen_repr() == PhpType::Void
                })
            })
            .map(|inst_id| (inst_id, slot))
            .collect()
    }

    /// DCE copies the tail into the `null` arm, so its read follows the null store in one block.
    #[test]
    fn null_store_in_the_same_block_proves_the_read() {
        let module = lower(
            r#"<?php
function jo(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; } var_dump($o); return 0; }
jo($argc);
"#,
        );
        let jo = function(&module, "jo");
        let loads = null_typed_loads(jo, "o");
        assert!(!loads.is_empty(), "the null arm reads $o typed null");
        for (load, slot) in loads {
            assert!(null_store_reaches_load(jo, load, slot, false));
        }
    }

    /// A read below an inner `if` of the null arm is proven through every predecessor.
    #[test]
    fn null_store_on_every_incoming_path_proves_the_read() {
        let module = lower(
            r#"<?php
function nested(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; if ($n > 1) { echo "deep"; } var_dump($o); return 1; } var_dump($o); return 0; }
nested($argc);
"#,
        );
        let nested = function(&module, "nested");
        let loads = null_typed_loads(nested, "o");
        assert!(!loads.is_empty(), "the null arm reads $o typed null");
        for (load, slot) in loads {
            assert!(null_store_reaches_load(nested, load, slot, false));
        }
    }

    /// Only one `switch` case stores `null`, so the read after the switch is reached by paths
    /// that still hold the object, and the proof must refuse it whatever the env says. Switch
    /// exits used to leave a stale `null` fact there; they now join every edge, so the read is
    /// no longer typed `null`, and the analysis is asked about it directly.
    #[test]
    fn a_read_reached_by_a_non_null_path_is_not_proven() {
        let module = lower(
            r#"<?php
function w(int $n) { $o = new stdClass(); switch ($n) { case 1: $o = null; break; case 2: echo "two"; break; } var_dump($o); return 0; }
w($argc);
"#,
        );
        let w = function(&module, "w");
        let slot = w
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some("o"))
            .expect("w: missing local $o")
            .id;
        let after_switch = w
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().copied())
            .filter(|inst_id| {
                w.instruction(*inst_id).is_some_and(|inst| {
                    inst.op == Op::LoadLocal
                        && inst.immediate == Some(Immediate::LocalSlot(slot))
                })
            })
            .last()
            .expect("w reads $o after the switch");
        assert!(!null_store_reaches_load(w, after_switch, slot, false));
        assert!(
            null_typed_loads(w, "o").iter().all(|(load, _)| *load != after_switch),
            "the switch exit no longer leaves a stale null fact for $o"
        );
    }

    /// A slot that may be a reference cell is never proven, whatever its stores say.
    #[test]
    fn a_reference_cell_slot_is_not_proven() {
        let module = lower(
            r#"<?php
function jo(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; } var_dump($o); return 0; }
jo($argc);
"#,
        );
        let jo = function(&module, "jo");
        for (load, slot) in null_typed_loads(jo, "o") {
            assert!(!null_store_reaches_load(jo, load, slot, true));
        }
    }
}
