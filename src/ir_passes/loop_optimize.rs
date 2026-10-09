//! Purpose:
//! Simplifies equivalent scalar induction variables and canonicalizes loop tests.
//!
//! Called from:
//! - The EIR fixed-point driver after scalar promotion and checked integer sinking.
//!
//! Key details:
//! - Coalesced pure recurrences must have identical overflow semantics; checked
//!   operations stay checked until integer range analysis proves them safe.
//! - Every latch must agree, including continue paths. Self-carried invariant
//!   parameters are replaced with their dominating preheader inputs.
//! - Exception handlers and suspended generators retain their original CFGs.

use std::collections::HashMap;
use crate::ir::{CmpPredicate, DataPool, Function, Immediate, Op, Ownership, PassOrigin, ValueDef, ValueId};
use super::cfg::has_exception_handlers;
use super::driver::IrPass;
use super::induction::{basic_inductions, branch_args_to, BasicInduction};
use super::rewrite::{defining_instruction, replace_all_uses};

/// Canonical induction and loop-test simplification.
pub struct LoopOptimize;

impl IrPass for LoopOptimize {
    /// Returns the stable diagnostic name.
    fn name(&self) -> &'static str { "loop-optimize" }

    /// Skips graphs without SSA parameters that could carry a loop counter.
    fn is_applicable(&self, function: &Function) -> bool {
        function.blocks.iter().any(|block| !block.params.is_empty())
    }

    /// Combines equivalent counters and puts induction operands first in comparisons.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function) || super::loop_cfg::has_suspension(function) {
            return false;
        }
        let dominance = super::dominance::compute_dominance(function);
        let loops = super::loops::compute_loops(function, &dominance);
        let mut changed = false;
        for lp in loops.loops() {
            changed |= fold_invariant_parameters(function, lp);
            let inductions = basic_inductions(function, lp, |value| integer_literal(function, value));
            let mut canonical: Vec<&BasicInduction> = Vec::new();
            let mut replacements = HashMap::new();
            for induction in &inductions {
                if let Some(previous) = canonical.iter().find(|previous| {
                    previous.step == induction.step
                        && same_initial(function, previous.initial, induction.initial)
                        && equivalent_updates(function, &dominance, previous, induction)
                }) {
                    replacements.insert(induction.parameter, previous.parameter);
                } else {
                    canonical.push(induction);
                }
            }
            if !replacements.is_empty() {
                replace_all_uses(function, &replacements);
                let removed: Vec<_> = function.blocks[lp.header.as_raw() as usize].params.iter()
                    .enumerate().filter_map(|(index, param)| replacements.contains_key(param).then_some(index)).collect();
                super::loop_cfg::remove_parameters(function, lp.header, &removed);
                changed = true;
            }
            for induction in canonical {
                // Normalize commuted updates so CSE can remove equivalent arithmetic.
                for &update in &induction.updates {
                    let Some(ValueDef::Instruction { inst, .. }) = function.value(update).map(|v| v.def) else { continue; };
                    let instruction = &mut function.instructions[inst.as_raw() as usize];
                    if instruction.op == Op::IAdd && instruction.operands[1] == induction.parameter {
                        instruction.operands.swap(0, 1);
                        instruction.origin = Some(PassOrigin::LoopOptimize);
                        changed = true;
                    }
                }
            }
            for &block in &lp.blocks {
                let instructions = function.blocks[block.as_raw() as usize].instructions.clone();
                for id in instructions {
                    let inst = &mut function.instructions[id.as_raw() as usize];
                    if inst.op != Op::ICmp || inst.operands.len() != 2 { continue; }
                    if inst.operands[0] == inst.operands[1] {
                        let Some(Immediate::CmpPredicate(predicate)) = inst.immediate else { continue; };
                        let truth = match predicate {
                            CmpPredicate::Eq | CmpPredicate::Sle | CmpPredicate::Sge => true,
                            CmpPredicate::Ne | CmpPredicate::Slt | CmpPredicate::Sgt => false,
                            _ => continue,
                        };
                        inst.op = Op::ConstBool;
                        inst.operands.clear();
                        inst.immediate = Some(Immediate::Bool(truth));
                        inst.effects = Op::ConstBool.default_effects();
                        inst.origin = Some(PassOrigin::LoopOptimize);
                        changed = true;
                        continue;
                    }
                    let is_induction = |value| inductions.iter().any(|iv| iv.parameter == value);
                    if is_induction(inst.operands[0]) || !is_induction(inst.operands[1]) { continue; }
                    let definition = function.values[inst.operands[0].as_raw() as usize].def;
                    let (ValueDef::Instruction { block: defined, .. } | ValueDef::BlockParam { block: defined, .. }) = definition;
                    if lp.contains(defined) { continue; }
                    let Some(Immediate::CmpPredicate(predicate)) = inst.immediate else { continue; };
                    let swapped = match predicate {
                        CmpPredicate::Eq | CmpPredicate::Ne => predicate,
                        CmpPredicate::Slt => CmpPredicate::Sgt,
                        CmpPredicate::Sle => CmpPredicate::Sge,
                        CmpPredicate::Sgt => CmpPredicate::Slt,
                        CmpPredicate::Sge => CmpPredicate::Sle,
                        _ => continue,
                    };
                    inst.operands.swap(0, 1);
                    inst.immediate = Some(Immediate::CmpPredicate(swapped));
                    inst.origin = Some(PassOrigin::LoopOptimize);
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Reads only a concrete signed-integer materialization.
fn integer_literal(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Compares initial SSA values, including independently materialized equal literals.
fn same_initial(function: &Function, left: ValueId, right: ValueId) -> bool {
    left == right || integer_literal(function, left).zip(integer_literal(function, right))
        .is_some_and(|(left, right)| left == right)
}

/// Requires equal recurrence semantics, including the overflow path of checked updates.
fn equivalent_updates(
    function: &Function,
    dominance: &super::dominance::DominanceInfo,
    left: &BasicInduction,
    right: &BasicInduction,
) -> bool {
    left.updates.iter().zip(&right.updates).all(|(left_value, right_value)| {
        let Some(left) = defining_instruction(function, *left_value) else { return false; };
        let Some(right) = defining_instruction(function, *right_value) else { return false; };
        let wrapping = |op| matches!(op, Op::IAdd | Op::ISub);
        if !left.effects.is_pure() || !right.effects.is_pure() { return false; }
        if wrapping(left.op) && wrapping(right.op) { return true; }
        if left.op != right.op || left.immediate != right.immediate { return false; }
        // Raw-slot checked updates can fatal despite their PURE metadata. Keep the
        // first check: removing an earlier check could move a fatal past output.
        let ValueDef::Instruction { block: left_block, index: left_index, .. } =
            function.values[left_value.as_raw() as usize].def else { return false; };
        let ValueDef::Instruction { block: right_block, index: right_index, .. } =
            function.values[right_value.as_raw() as usize].def else { return false; };
        if left_block == right_block { left_index <= right_index }
        else { dominance.dominates(left_block, right_block) }
    })
}

/// Replaces a scalar header parameter unchanged on every latch with its preheader input.
fn fold_invariant_parameters(function: &mut Function, lp: &super::loops::NaturalLoop) -> bool {
    let Some(initial) = lp.preheader.and_then(|pre| branch_args_to(function, pre, lp.header)) else {
        return false;
    };
    let mut replacements = HashMap::new();
    let mut removed = Vec::new();
    for (index, &parameter) in function.blocks[lp.header.as_raw() as usize].params.iter().enumerate() {
        let Some(&initial) = initial.get(index) else { continue; };
        let value = &function.values[parameter.as_raw() as usize];
        let incoming = &function.values[initial.as_raw() as usize];
        if value.ownership != Ownership::NonHeap || value.ir_type != incoming.ir_type
            || value.php_type != incoming.php_type || value.ownership != incoming.ownership
        { continue; }
        if lp.latches.iter().all(|&latch| branch_args_to(function, latch, lp.header)
            .and_then(|args| args.get(index).copied())
            .is_some_and(|value| value == parameter || value == initial))
        {
            replacements.insert(parameter, initial);
            removed.push(index);
        }
    }
    if replacements.is_empty() { return false; }
    replace_all_uses(function, &replacements);
    super::loop_cfg::remove_parameters(function, lp.header, &removed);
    true
}
