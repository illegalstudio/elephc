//! Purpose:
//! Hoists `strlen(gzcompress($s))` / `strlen(gzdeflate($s))` out of a loop when the
//! subject local is assigned once and never rewritten, so the compressed length is
//! computed once instead of on every iteration.
//!
//! Called from:
//! - The EIR fixed-point pass driver after loop simplification.
//!
//! Key details:
//! - The slot must have exactly one entry store in the whole function; every other use is a
//!   load or scope cleanup. Any store, reference alias, include, or eval keeps the call in place.
//! - The compressed value may be observed only by this `strlen` and one release. Both move
//!   with the call so ownership stays balanced and the loop never releases a preheader value.
//! - The optional compression level must be a valid literal or defined outside the loop, so
//!   moving the call cannot introduce a dominance violation or a PHP ValueError.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, LocalKind, Op, PassOrigin,
    RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::cfg::has_exception_handlers;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::licm::{build_def_block, move_instruction, recompute_instruction_defs};
use super::loops::{compute_loops, NaturalLoop};

/// Hoists an invariant gzip length computation into the loop preheader.
pub struct GzLenHoist;

impl IrPass for GzLenHoist {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "gz_len_hoist"
    }

    /// Skips functions with no string length or compression call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            inst.op == Op::StrLen || matches!(direct_target(inst), Some(RuntimeFnId::Gzcompress | RuntimeFnId::Gzdeflate))
        })
    }

    /// Hoists every independently proven length pair.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || super::local_scope::has_dynamic_eval(function)
        {
            return false;
        }
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }
        let mut order: Vec<usize> = (0..loops.loops().len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(loops.loops()[index].depth));
        let mut def_block = build_def_block(function);
        let mut changed = false;
        for index in order {
            let loop_ref = &loops.loops()[index];
            let Some(preheader) = loop_ref.preheader else {
                continue;
            };
            for (load, call, length, release) in find_moves(function, loop_ref, &def_block) {
                move_instruction(function, load, preheader, &mut def_block);
                move_instruction(function, call, preheader, &mut def_block);
                move_instruction(function, length, preheader, &mut def_block);
                move_release(function, release, preheader);
                for id in [load, call, length] {
                    if let Some(inst) = function.instruction_mut(id) {
                        inst.origin = Some(PassOrigin::Licm);
                    }
                }
                changed = true;
            }
        }
        if changed {
            recompute_instruction_defs(function);
        }
        changed
    }
}

/// Returns the direct or profiled runtime target for one call.
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Returns `(subject load, gzip call, str_len, release)` groups safe to move together.
fn find_moves(
    function: &Function,
    loop_ref: &NaturalLoop,
    def_block: &HashMap<ValueId, BlockId>,
) -> Vec<(InstId, InstId, InstId, InstId)> {
    let loop_blocks: HashSet<TargetBlock> = loop_ref.blocks.iter().copied().map(TargetBlock).collect();
    let mut moves = Vec::new();
    for &block_id in &loop_ref.blocks {
        let Some(block) = function.block(block_id) else {
            continue;
        };
        for &length_id in &block.instructions {
            let Some(length) = function.instruction(length_id) else {
                continue;
            };
            if length.op != Op::StrLen || length.operands.len() != 1 {
                continue;
            }
            let Some(call_id) = defining_id(function, length.operands[0]) else {
                continue;
            };
            let Some(call) = function.instruction(call_id) else {
                continue;
            };
            let Some(target) = direct_target(call) else {
                continue;
            };
            if !matches!(target, RuntimeFnId::Gzcompress | RuntimeFnId::Gzdeflate)
                || !matches!(call.operands.len(), 1 | 2)
            {
                continue;
            }
            let Some(&subject) = call.operands.first() else {
                continue;
            };
            let Some(load_id) = defining_id(function, subject) else {
                continue;
            };
            let Some(load) = function.instruction(load_id) else {
                continue;
            };
            if load.op != Op::LoadLocal {
                continue;
            }
            let Some(Immediate::LocalSlot(slot)) = load.immediate else {
                continue;
            };
            if !slot_is_whole_function_invariant(function, slot) {
                continue;
            }
            // Any extra compression argument (the level) must already dominate the preheader.
            if call.operands.iter().skip(1).any(|operand| {
                def_block
                    .get(operand)
                    .is_some_and(|block| loop_blocks.contains(&TargetBlock(*block)))
            }) {
                continue;
            }
            if call.operands.len() == 2 && !valid_level(function, call.operands[1]) {
                continue;
            }
            let Some(call_result) = call.result else {
                continue;
            };
            // Exactly one release of the compressed string, inside this loop and after the call.
            let mut release = None;
            let mut valid = true;
            for &other_block in &loop_ref.blocks {
                let Some(other) = function.block(other_block) else {
                    valid = false;
                    break;
                };
                for &id in &other.instructions {
                    let Some(inst) = function.instruction(id) else {
                        valid = false;
                        break;
                    };
                    if inst.operands.contains(&call_result) {
                        if inst.op == Op::Release && inst.operands.len() == 1 {
                            if release.replace(id).is_some() {
                                valid = false;
                                break;
                            }
                        } else if id != call_id && id != length_id {
                            valid = false;
                            break;
                        }
                    }
                }
                if !valid {
                    break;
                }
            }
            if !valid || release.is_none() {
                continue;
            }
            // Only one def site per value keeps the group unambiguous.
            if def_block.get(&length.operands[0]) != Some(&block_id)
                || def_block.get(&subject) != Some(&block_id)
            {
                continue;
            }
            moves.push((load_id, call_id, length_id, release.unwrap()));
        }
    }
    moves
}

/// Wrapper so `BlockId` can live in a set without importing another trait.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TargetBlock(BlockId);

/// Requires a single entry store and only reads everywhere else in the function.
fn slot_is_whole_function_invariant(function: &Function, slot: crate::ir::LocalSlotId) -> bool {
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    if local.kind != LocalKind::PhpLocal
        || local.php_type.codegen_repr() != PhpType::Str
    {
        return false;
    }
    let Some(entry) = function.blocks.first() else {
        return false;
    };
    let mut stored = false;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot) {
            continue;
        }
        match inst.op {
            Op::StoreLocal if inst.operands.len() == 1 => {
                if stored || !entry.instructions.contains(&InstId::from_raw(raw as u32)) {
                    return false;
                }
                stored = true;
            }
            Op::LoadLocal | Op::ReleaseLocalSlot => {}
            _ => return false,
        }
    }
    stored
}

/// Returns whether the optional level is PHP's default or an integer in the valid range.
fn valid_level(function: &Function, value: ValueId) -> bool {
    let Some(inst) = defining_instruction(function, value) else {
        return false;
    };
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(level))) => (-1..=9).contains(level),
        _ => false,
    }
}

/// Moves one release instruction (which has no result) into the preheader.
fn move_release(function: &mut Function, release: InstId, preheader: BlockId) {
    let mut origin = None;
    for block in &mut function.blocks {
        if let Some(position) = block.instructions.iter().position(|&id| id == release) {
            block.instructions.remove(position);
            origin = Some(block.id);
            break;
        }
    }
    if origin.is_some() {
        if let Some(block) = function.block_mut(preheader) {
            block.instructions.push(release);
        }
    }
}

/// Returns the defining instruction identifier for one SSA value.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some(inst)
}

/// Returns the defining instruction for one SSA value.
fn defining_instruction(function: &Function, value: ValueId) -> Option<&crate::ir::Instruction> {
    let id = defining_id(function, value)?;
    function.instruction(id)
}
