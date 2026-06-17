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

use std::collections::HashMap;

use crate::ir::{Function, Terminator, ValueId};

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
