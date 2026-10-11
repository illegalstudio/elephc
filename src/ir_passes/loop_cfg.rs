//! Purpose:
//! Removes intermediate update blocks on hot natural-loop paths.
//!
//! Called from:
//! - Branch simplification after threading, and induction-variable coalescing.
//!
//! Key details:
//! - Merges only unconditional, single-predecessor edges within the same loop.
//! - Headers survive so cleanup cannot undo LICM or change loop entry semantics.
//! - Retired parameters keep stable value IDs through unused Nop definitions.

use std::collections::HashMap;
use crate::ir::{BlockId, Function, InstId, Instruction, Op, Terminator, ValueDef};
use super::cfg::{predecessors, successor_edges};
use super::rewrite::replace_all_uses;

/// Detects implicit generator re-entry boundaries that CFG merging must preserve.
pub(super) fn has_suspension(function: &Function) -> bool {
    function.blocks.iter().any(|block| matches!(block.terminator, Some(Terminator::GeneratorSuspend { .. })))
}

/// Merges one sweep of straight-line loop blocks, retaining headers and stable IDs.
pub(super) fn merge_loop_blocks(function: &mut Function) -> bool {
    if has_suspension(function) { return false; }
    let dominance = super::dominance::compute_dominance(function);
    let loops = super::loops::compute_loops(function, &dominance);
    let mut preds = predecessors(function);
    let mut changed = false;
    for index in 0..function.blocks.len() {
        loop {
            let source = function.blocks[index].id;
            let Some(Terminator::Br { target, args }) = function.blocks[index].terminator.clone() else { break; };
            if target == source || target == function.entry || loops.is_loop_header(target) { break; }
            let Some(lp) = loops.innermost_loop(source) else { break; };
            if loops.innermost_loop(target).map(|other| other.header) != Some(lp.header) { break; }
            // Include unreachable predecessors: their SSA uses must stay well-formed too.
            if preds[target.as_raw() as usize].as_slice() != [source] { break; }
            let params = function.blocks[target.as_raw() as usize].params.clone();
            let replacements = params.iter().copied().zip(args).collect::<HashMap<_, _>>();
            replace_all_uses(function, &replacements);
            remove_parameters(function, target, &(0..params.len()).collect::<Vec<_>>());
            let moved = std::mem::take(&mut function.blocks[target.as_raw() as usize].instructions);
            let terminator = function.blocks[target.as_raw() as usize].terminator.take();
            function.blocks[target.as_raw() as usize].terminator = Some(Terminator::Unreachable);
            function.blocks[index].instructions.extend(moved);
            function.blocks[index].terminator = terminator;
            for (position, &inst) in function.blocks[index].instructions.iter().enumerate() {
                if let Some(result) = function.instructions[inst.as_raw() as usize].result {
                    function.values[result.as_raw() as usize].def = ValueDef::Instruction {
                        block: source, index: position as u32, inst,
                    };
                }
            }
            preds = predecessors(function);
            changed = true;
        }
    }
    changed
}

/// Removes selected parameter positions and matching arguments while preserving dead value IDs.
pub(super) fn remove_parameters(function: &mut Function, block: BlockId, removed: &[usize]) {
    let old = function.blocks[block.as_raw() as usize].params.clone();
    let mut kept = Vec::new();
    for (index, parameter) in old.into_iter().enumerate() {
        if removed.contains(&index) {
            let value = &mut function.values[parameter.as_raw() as usize];
            let inst = InstId::from_raw(function.instructions.len() as u32);
            let position = function.blocks[block.as_raw() as usize].instructions.len();
            function.instructions.push(Instruction::new(
                Op::Nop, Vec::new(), None, Some(parameter), value.ir_type,
                value.php_type.clone(), value.ownership, Op::Nop.default_effects(), None,
            ));
            function.blocks[block.as_raw() as usize].instructions.push(inst);
            value.def = ValueDef::Instruction { block, index: position as u32, inst };
        } else {
            function.values[parameter.as_raw() as usize].def = ValueDef::BlockParam {
                block, index: kept.len() as u16,
            };
            kept.push(parameter);
        }
    }
    function.blocks[block.as_raw() as usize].params = kept;
    for basic_block in &mut function.blocks {
        if let Some(term) = &mut basic_block.terminator {
            for (target, args) in successor_edges(term) {
                if *target == block {
                    let mut index = 0;
                    args.retain(|_| { let keep = !removed.contains(&index); index += 1; keep });
                }
            }
        }
    }
}
