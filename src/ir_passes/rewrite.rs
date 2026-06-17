//! Purpose:
//! Shared use-rewriting (RAUW) helper for mutating EIR passes. Replaces every
//! *use* of a set of value ids with their mapped replacements.
//!
//! Called from:
//! - `crate::ir_passes::identity_arith` and future transformation passes
//!   (peephole, constant propagation, CSE) that need to redirect value uses.
//!
//! Key details:
//! - Only *uses* are rewritten: instruction operands and every `ValueId` slot
//!   carried by a terminator. Definitions (block parameters and instruction
//!   results) are intentionally left untouched.
//! - The traversal mirrors the read-only `terminator_uses` walk in
//!   `crate::ir_passes::liveness` so the two stay in lockstep as the terminator
//!   set evolves.
//! - Shared fold helpers (`resolve_chains`, `neutralize_to_nop`,
//!   `defining_instruction`) live here so the identity-arith and peephole passes
//!   share one dominance-safe RAUW + neutralization model.

use std::collections::HashMap;

use crate::ir::{Function, Instruction, Op, Terminator, ValueDef, ValueId};

/// Replaces every use of each mapped `ValueId` across all instruction operands
/// and all terminators in the function. The map should already be resolved so
/// its values are terminal (not themselves keys); callers that fold chains must
/// resolve them first. Block parameters and instruction results are not touched.
pub(crate) fn replace_all_uses(function: &mut Function, map: &HashMap<ValueId, ValueId>) {
    if map.is_empty() {
        return;
    }
    for inst in function.instructions.iter_mut() {
        for operand in inst.operands.iter_mut() {
            remap(operand, map);
        }
    }
    for block in function.blocks.iter_mut() {
        if let Some(terminator) = block.terminator.as_mut() {
            replace_in_terminator(terminator, map);
        }
    }
}

/// Resolves a fold-to-operand map so each value maps to its terminal target,
/// chasing chains like `a -> b -> x` down to `x`. SSA forward-definition ordering
/// makes cycles impossible; the length guard is a defensive backstop.
pub(crate) fn resolve_chains(raw: &HashMap<ValueId, ValueId>) -> HashMap<ValueId, ValueId> {
    let mut resolved = HashMap::with_capacity(raw.len());
    for (&key, &start) in raw {
        let mut target = start;
        let mut steps = 0;
        while let Some(&next) = raw.get(&target) {
            target = next;
            steps += 1;
            if steps > raw.len() {
                break;
            }
        }
        resolved.insert(key, target);
    }
    resolved
}

/// Neutralizes a folded instruction into a `Nop`, preserving its result value so
/// the value table stays consistent while clearing operands, immediate, and
/// effects to match `Nop`.
pub(crate) fn neutralize_to_nop(inst: &mut Instruction) {
    inst.op = Op::Nop;
    inst.operands.clear();
    inst.immediate = None;
    inst.effects = Op::Nop.default_effects();
}

/// Counts how many times each value is *used* across all instruction operands
/// and terminator slots. Definitions are not counted. Used by peephole patterns
/// that act only on single-use values (e.g. paired acquire/release).
pub(crate) fn count_value_uses(function: &Function) -> HashMap<ValueId, usize> {
    let mut counts: HashMap<ValueId, usize> = HashMap::new();
    for inst in &function.instructions {
        for &operand in &inst.operands {
            *counts.entry(operand).or_insert(0) += 1;
        }
    }
    for block in &function.blocks {
        if let Some(terminator) = block.terminator.as_ref() {
            for value in super::liveness::terminator_uses(terminator) {
                *counts.entry(value).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// Returns the instruction that defines `value`, if it is instruction-defined.
pub(crate) fn defining_instruction(function: &Function, value: ValueId) -> Option<&Instruction> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    function.instruction(inst)
}

/// Rewrites a single value slot in place if it appears in the replacement map.
fn remap(value: &mut ValueId, map: &HashMap<ValueId, ValueId>) {
    if let Some(&replacement) = map.get(value) {
        *value = replacement;
    }
}

/// Rewrites every value used by a terminator: branch arguments, conditions,
/// switch scrutinees, returned/thrown values, and generator suspend operands.
fn replace_in_terminator(terminator: &mut Terminator, map: &HashMap<ValueId, ValueId>) {
    match terminator {
        Terminator::Br { args, .. } => {
            for arg in args.iter_mut() {
                remap(arg, map);
            }
        }
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => {
            remap(cond, map);
            for arg in then_args.iter_mut() {
                remap(arg, map);
            }
            for arg in else_args.iter_mut() {
                remap(arg, map);
            }
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => {
            remap(scrutinee, map);
            for case in cases.iter_mut() {
                for arg in case.args.iter_mut() {
                    remap(arg, map);
                }
            }
            for arg in default_args.iter_mut() {
                remap(arg, map);
            }
        }
        Terminator::Return { value } => {
            if let Some(value) = value.as_mut() {
                remap(value, map);
            }
        }
        Terminator::Throw { value } => remap(value, map),
        Terminator::GeneratorSuspend {
            key,
            value,
            resume_args,
            ..
        } => {
            if let Some(key) = key.as_mut() {
                remap(key, map);
            }
            if let Some(value) = value.as_mut() {
                remap(value, map);
            }
            for arg in resume_args.iter_mut() {
                remap(arg, map);
            }
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => {}
    }
}
