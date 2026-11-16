//! Purpose:
//! Caches stable inlined accessor properties behind the loop's first actual entry checks.
//!
//! Called from:
//! - `buffer_property_loops`, before ordinary buffer property and descriptor hoisting.
//!
//! Key details:
//! - The first scalar header and checked prefix run in their original order on a taken entry.
//! - Cold terminal throws are cloned with initial SSA operands and the original owner slots.
//! - Only read-only loops qualify; zero-trip exits and instructions' lexical scopes are preserved.

use std::collections::{HashMap, HashSet};

use crate::ir::{BasicBlock, BlockId, Function, Immediate, InstId, Op, Terminator, ValueDef, ValueId};
use crate::types::PhpType;

use super::buffer_property_loops::{append_clone, can_duplicate, guard_entry, repair_definitions};
use super::buffer_property_receivers::{can_load_receiver, invariant_receiver};
use super::buffer_property_reuse::{is_buffer_property_read, is_inlined_scalar_property_read, keeps_reads_stable};
use super::loops::NaturalLoop;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// One straight-line prefix block and its optional terminal failure edge.
struct Prefix {
    instructions: Vec<InstId>,
    next: Option<(ValueId, BlockId, bool)>,
}

/// Caches the first guarded buffer property's invariant prefix, at most once per property.
pub(super) fn hoist(function: &mut Function, loop_ref: &NaturalLoop) -> bool {
    let Some((entry, split)) = entry_edge(function, loop_ref) else { return false; };
    if !loop_ref.blocks.iter().all(|&block| function.blocks[block.as_raw() as usize]
        .instructions.iter().all(|id| keeps_reads_stable(function, &function.instructions[id.as_raw() as usize])))
    { return false; }
    let header = &function.blocks[loop_ref.header.as_raw() as usize];
    let Some(Terminator::CondBr { then_target, else_target, .. }) = header.terminator else { return false; };
    let (body, exit) = match (loop_ref.contains(then_target), loop_ref.contains(else_target)) {
        (true, false) => (then_target, else_target),
        (false, true) => (else_target, then_target),
        _ => return false,
    };
    if !header.instructions.iter().all(|&id| can_duplicate(function, id))
        || header_values_escape(function, loop_ref, exit)
    { return false; }
    let Some(prefix) = collect_prefix(function, loop_ref, body, entry) else { return false; };
    let preheader = if let Some((taken, args)) = split {
        let preheader = new_block(function, "buffer_guard_preheader");
        function.blocks[preheader.as_raw() as usize].terminator = Some(Terminator::Br { target: loop_ref.header, args });
        let Some(Terminator::CondBr { then_target, then_args, else_target, else_args, .. }) =
            function.blocks[entry.as_raw() as usize].terminator.as_mut() else { unreachable!(); };
        if taken { *then_target = preheader; then_args.clear(); }
        else { *else_target = preheader; else_args.clear(); }
        preheader
    } else { entry };
    let Some((guarded, mut replacements)) = guard_entry(function, loop_ref, preheader, body) else { return false; };
    let tail = function.blocks[guarded.as_raw() as usize].terminator.take().unwrap();
    let mut current = guarded;
    let mut cached = HashMap::new();
    let mut receiver_loads = HashMap::new();
    let mut invariants = HashMap::new();
    for part in prefix {
        for id in part.instructions {
            let inst = &function.instructions[id.as_raw() as usize];
            let original = inst.result;
            if inst.op == Op::LoadLocal && matches!(inst.result_php_type, PhpType::Object(_)) {
                if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                    if let Some(&value) = receiver_loads.get(&slot) {
                        replacements.insert(original.unwrap(), value);
                        cached.insert(original.unwrap(), value);
                        continue;
                    }
                }
            }
            let invariant = original.is_some_and(|value| invariant_value(function, loop_ref, value, &mut invariants));
            let value = clone_inst(function, current, id, &mut replacements);
            if let (Some(original), Some(value)) = (original, value) {
                let inst = &function.instructions[id.as_raw() as usize];
                if invariant { cached.insert(original, value); }
                if inst.op == Op::LoadLocal && matches!(inst.result_php_type, PhpType::Object(_)) {
                    if let Some(Immediate::LocalSlot(slot)) = inst.immediate { receiver_loads.insert(slot, value); }
                }
            }
        }
        if let Some((condition, failure, failure_when_true)) = part.next {
            let failed = new_block(function, "buffer_guard_failure");
            let mut cold_map = replacements.clone();
            let cold = function.blocks[failure.as_raw() as usize].clone();
            for id in cold.instructions { clone_inst(function, failed, id, &mut cold_map); }
            let mut terminal = cold.terminator.unwrap();
            super::rewrite::replace_in_terminator(&mut terminal, &cold_map);
            function.blocks[failed.as_raw() as usize].terminator = Some(terminal);
            let next = new_block(function, "buffer_guard_prefix");
            let cond = replacements.get(&condition).copied().unwrap_or(condition);
            function.blocks[current.as_raw() as usize].terminator = Some(Terminator::CondBr {
                cond,
                then_target: if failure_when_true { failed } else { next }, then_args: vec![],
                else_target: if failure_when_true { next } else { failed }, else_args: vec![],
            });
            current = next;
        }
    }
    function.blocks[current.as_raw() as usize].terminator = Some(tail);
    replace_all_uses(function, &cached);
    for original in cached.keys() {
        if let ValueDef::Instruction { inst, .. } = function.values[original.as_raw() as usize].def {
            neutralize_to_nop(&mut function.instructions[inst.as_raw() as usize]);
        }
    }
    repair_definitions(function);
    true
}

/// Selects an existing preheader or the unique conditional entry edge that can be split.
fn entry_edge(function: &Function, loop_ref: &NaturalLoop) -> Option<(BlockId, Option<(bool, Vec<ValueId>)>)> {
    if let Some(preheader) = loop_ref.preheader { return Some((preheader, None)); }
    let dominance = super::dominance::compute_dominance(function);
    let predecessors = super::cfg::predecessors(function);
    let outside: HashSet<_> = predecessors[loop_ref.header.as_raw() as usize].iter().copied()
        .filter(|&block| !loop_ref.contains(block) && dominance.is_reachable(block)).collect();
    if outside.len() != 1 { return None; }
    let source = *outside.iter().next()?;
    let Terminator::CondBr { then_target, then_args, else_target, else_args, .. } =
        function.block(source)?.terminator.as_ref()? else { return None; };
    match (*then_target == loop_ref.header, *else_target == loop_ref.header) {
        (true, false) => Some((source, Some((true, then_args.clone())))),
        (false, true) => Some((source, Some((false, else_args.clone())))),
        _ => None,
    }
}

/// Collects a bounded entry chain ending at an inlined invariant buffer-property read.
fn collect_prefix(function: &Function, loop_ref: &NaturalLoop, body: BlockId, preheader: BlockId) -> Option<Vec<Prefix>> {
    let dominance = super::dominance::compute_dominance(function);
    let mut available: HashSet<_> = function.blocks[loop_ref.header.as_raw() as usize].params.iter().copied().collect();
    available.extend(function.blocks[loop_ref.header.as_raw() as usize].instructions.iter()
        .filter_map(|id| function.instructions[id.as_raw() as usize].result));
    let mut prefix = Vec::new();
    let mut seen = HashSet::new();
    let mut block = body;
    let mut count = 0;
    loop {
        if !seen.insert(block) || seen.len() > 16 { return None; }
        let basic = function.block(block)?;
        if !basic.params.is_empty() { return None; }
        let mut instructions = Vec::new();
        for &id in &basic.instructions {
            let inst = &function.instructions[id.as_raw() as usize];
            if inst.op == Op::Nop { continue; }
            count += 1;
            if count > 96 { return None; }
            let property = inst.op == Op::PropGet
                && (is_buffer_property_read(function, inst) || is_inlined_scalar_property_read(function, inst))
                && function.inlined_scopes.contains_key(&id)
                && inst.operands.first().is_some_and(|&receiver| invariant_receiver(function, loop_ref, receiver));
            if !(can_duplicate(function, id) || property
                || (inst.op == Op::IsNull && inst.operands.first().is_some_and(|&value| invariant_receiver(function, loop_ref, value))))
                || !inst.operands.iter().all(|value| available.contains(value) || value_block(function, *value)
                    .is_some_and(|block| !loop_ref.contains(block) && dominance.dominates(block, preheader)))
            { return None; }
            instructions.push(id);
            available.extend(inst.result);
            if property && matches!(inst.result_php_type, PhpType::Buffer(_)) {
                prefix.push(Prefix { instructions, next: None });
                return Some(prefix);
            }
        }
        let Terminator::CondBr { cond, then_target, then_args, else_target, else_args } = basic.terminator.as_ref()? else { return None; };
        if !then_args.is_empty() || !else_args.is_empty() { return None; }
        let (success, failure, failure_when_true) = match (loop_ref.contains(*then_target), loop_ref.contains(*else_target)) {
            (true, false) => (*then_target, *else_target, false),
            (false, true) => (*else_target, *then_target, true),
            _ => return None,
        };
        let cold = function.block(failure)?;
        if !cold.params.is_empty() || cold.instructions.len() > 128
            || !matches!(cold.terminator, Some(Terminator::Throw { .. } | Terminator::Unreachable))
        { return None; }
        let mut cold_available = available.clone();
        for &id in &cold.instructions {
            let inst = &function.instructions[id.as_raw() as usize];
            if !inst.operands.iter().all(|value| cold_available.contains(value) || value_block(function, *value)
                .is_some_and(|block| !loop_ref.contains(block) && dominance.dominates(block, preheader)))
            { return None; }
            cold_available.extend(inst.result);
        }
        if !cold.terminator.as_ref()?.used_values().iter().all(|value| cold_available.contains(value)
            || value_block(function, *value).is_some_and(|block| !loop_ref.contains(block) && dominance.dominates(block, preheader)))
        { return None; }
        prefix.push(Prefix { instructions, next: Some((*cond, failure, failure_when_true)) });
        block = success;
    }
}

/// Proves an immutable scalar prefix value without treating loop-carried indexes as invariant.
fn invariant_value(function: &Function, loop_ref: &NaturalLoop, value: ValueId, memo: &mut HashMap<ValueId, bool>) -> bool {
    if let Some(&invariant) = memo.get(&value) { return invariant; }
    memo.insert(value, false);
    let Some(value_info) = function.value(value) else { return false; };
    let ValueDef::Instruction { block, inst, .. } = value_info.def else {
        let invariant = value_block(function, value).is_some_and(|block| !loop_ref.contains(block));
        memo.insert(value, invariant);
        return invariant;
    };
    if !loop_ref.contains(block) { memo.insert(value, true); return true; }
    let inst_info = &function.instructions[inst.as_raw() as usize];
    let invariant = if inst_info.op == Op::LoadLocal {
        can_load_receiver(function, inst) && invariant_receiver(function, loop_ref, value)
    } else {
        inst_info.operands.iter().all(|&value| invariant_value(function, loop_ref, value, memo))
    };
    memo.insert(value, invariant);
    invariant
}

/// Rejects a new zero-trip edge that could bypass header definitions observed after the loop.
fn header_values_escape(function: &Function, loop_ref: &NaturalLoop, exit: BlockId) -> bool {
    let header = &function.blocks[loop_ref.header.as_raw() as usize];
    let values: HashSet<_> = header.params.iter().copied().chain(header.instructions.iter()
        .filter_map(|id| function.instructions[id.as_raw() as usize].result)).collect();
    let mut work = vec![exit];
    let mut seen = HashSet::new();
    while let Some(block) = work.pop() {
        if loop_ref.contains(block) || !seen.insert(block) { continue; }
        let basic = &function.blocks[block.as_raw() as usize];
        if basic.instructions.iter().any(|id| function.instructions[id.as_raw() as usize]
            .operands.iter().any(|value| values.contains(value)))
            || basic.terminator.as_ref().is_some_and(|term| term.used_values().iter().any(|value| values.contains(value)))
        { return true; }
        if let Some(term) = &basic.terminator { work.extend(super::cfg::successors(term)); }
    }
    false
}

/// Returns the block defining an instruction result or block parameter.
fn value_block(function: &Function, value: ValueId) -> Option<BlockId> {
    Some(match function.value(value)?.def { ValueDef::Instruction { block, .. } | ValueDef::BlockParam { block, .. } => block })
}

/// Appends a remapped clone while retaining lexical property and method scope metadata.
fn clone_inst(function: &mut Function, block: BlockId, id: InstId, replacements: &mut HashMap<ValueId, ValueId>) -> Option<ValueId> {
    let mut inst = function.instructions[id.as_raw() as usize].clone();
    let old = inst.result;
    for value in &mut inst.operands { *value = replacements.get(value).copied().unwrap_or(*value); }
    let clone = InstId::from_raw(function.instructions.len() as u32);
    let result = append_clone(function, block, inst);
    if let Some(scope) = function.inlined_scopes.get(&id).cloned() { function.inlined_scopes.insert(clone, scope); }
    if let (Some(old), Some(result)) = (old, result) { replacements.insert(old, result); }
    result
}

/// Creates a guarded entry block without renumbering existing blocks.
fn new_block(function: &mut Function, name: &str) -> BlockId {
    let id = BlockId::from_raw(function.blocks.len() as u32);
    function.blocks.push(BasicBlock::new(id, format!("{name}_{}", id.as_raw()), vec![]));
    id
}
