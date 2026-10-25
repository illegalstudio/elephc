//! Purpose:
//! Moves stable buffer-property reads into loop preheaders so descriptor resolution can follow.
//!
//! Called from:
//! - `buffer_access::optimize_function`, under its function and module safety gates.
//!
//! Key details:
//! - Only named reads on the method receiver and loops without calls or property writes qualify.
//! - A body read needs a duplicated scalar entry condition, preserving zero-trip behavior.
//! - Conditional reads, faulting prefixes and header values used after the loop fail closed.
//! - Property instructions are relocated with their original ids, spans and lexical metadata.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BasicBlock, BlockId, Effects, Function, Immediate, InstId, Instruction, LocalSlotId,
    Op, Ownership, PassOrigin, Terminator, Value, ValueDef, ValueId,
};

use super::buffer_access::buffer_handle_value;
use super::buffer_property_reuse::{is_buffer_property_read, keeps_reads_stable, reuse_property_reads};
use super::dominance::compute_dominance;
use super::loops::{compute_loops, NaturalLoop};

/// Hoists one guaranteed property read at a time, recomputing CFG facts after guarded entries.
pub(super) fn hoist_property_reads(function: &mut Function) -> bool {
    let mut changed = false;
    loop {
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        let mut order: Vec<_> = loops.loops().iter().collect();
        order.sort_by_key(|loop_ref| loop_ref.depth);
        let moved = order.into_iter().any(|loop_ref| hoist_one(function, loop_ref));
        if !moved { return changed; }
        changed = true;
        // Merge before resolving descriptors, so later body accesses still reach loop hoisting.
        reuse_property_reads(function);
    }
}

/// Relocates the first buffer read of a clean loop header or its unconditional body entry.
fn hoist_one(function: &mut Function, loop_ref: &NaturalLoop) -> bool {
    let Some(preheader) = loop_ref.preheader else { return false; };
    if !loop_ref.blocks.iter().all(|&block| {
        function.blocks[block.as_raw() as usize].instructions.iter().all(|&id| {
            keeps_reads_stable(function, &function.instructions[id.as_raw() as usize])
        })
    }) { return false; }
    let header = loop_ref.header;
    let header_insts = function.blocks[header.as_raw() as usize].instructions.clone();
    if let Some((position, &read)) = header_insts.iter().enumerate().find(|(_, id)| {
        is_buffer_property_read(function, &function.instructions[id.as_raw() as usize])
    }) {
        if header_insts[..position].iter().all(|&id| can_duplicate(function, id))
            && feeds_access(function, loop_ref, read)
        {
            move_read(function, header, read, preheader);
            return true;
        }
        return false;
    }
    let Some(body) = body_entry(function, loop_ref) else { return false; };
    let body_insts = function.blocks[body.as_raw() as usize].instructions.clone();
    let Some((position, &read)) = body_insts.iter().enumerate().find(|(_, id)| {
        is_buffer_property_read(function, &function.instructions[id.as_raw() as usize])
    }) else { return false; };
    if !header_insts.iter().all(|&id| can_duplicate(function, id))
        || !body_insts[..position].iter().all(|&id| can_duplicate(function, id))
        || !feeds_access(function, loop_ref, read)
        || header_values_escape(function, loop_ref)
    { return false; }
    let Some(guarded) = guard_entry(function, loop_ref, preheader, body) else { return false; };
    move_read(function, body, read, guarded);
    true
}

/// Returns the sole in-loop successor of a conditional header with one exit edge.
fn body_entry(function: &Function, loop_ref: &NaturalLoop) -> Option<BlockId> {
    let Terminator::CondBr { then_target, else_target, .. } =
        function.block(loop_ref.header)?.terminator.as_ref()? else { return None; };
    match (loop_ref.contains(*then_target), loop_ref.contains(*else_target)) {
        (true, false) if *then_target != loop_ref.header => Some(*then_target),
        (false, true) if *else_target != loop_ref.header => Some(*else_target),
        _ => None,
    }
}

/// Requires at least one raw buffer access using this property's handle in the same loop.
fn feeds_access(function: &Function, loop_ref: &NaturalLoop, read: InstId) -> bool {
    let Some(handle) = function.instructions[read.as_raw() as usize].result else { return false; };
    loop_ref.blocks.iter().any(|&block| {
        function.blocks[block.as_raw() as usize].instructions.iter().any(|&id| {
            let inst = &function.instructions[id.as_raw() as usize];
            matches!(inst.op, Op::BufferGet | Op::BufferSet)
                && inst.operands.first().is_some_and(|&value| buffer_handle_value(function, value) == handle)
        })
    })
}

/// Allows only total scalar calculations and exact local loads, including borrowed `$this`.
fn can_duplicate(function: &Function, id: InstId) -> bool {
    let inst = &function.instructions[id.as_raw() as usize];
    if inst.op == Op::LoadLocal {
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return false; };
        let Some(local) = function.locals.get(slot.as_raw() as usize) else { return false; };
        return inst.effects.difference(Effects::READS_LOCAL).is_empty()
            && local.ir_type == inst.result_type
            && local.php_type == inst.result_php_type
            && (inst.result_ownership == Ownership::NonHeap
                || (slot == LocalSlotId::from_raw(0) && function.flags.is_method
                    && matches!(inst.result_ownership, Ownership::Borrowed | Ownership::MaybeOwned)));
    }
    inst.effects.is_pure()
        && matches!(inst.result_ownership, Ownership::NonHeap | Ownership::Persistent)
        && matches!(inst.op,
            Op::Nop | Op::ConstI64 | Op::ConstF64 | Op::ConstBool | Op::ConstNull
                | Op::ICmp | Op::FCmp | Op::IsTruthy | Op::IAdd | Op::ISub | Op::IMul
                | Op::IBitAnd | Op::IBitOr | Op::IBitXor | Op::IBitNot | Op::INeg
                | Op::IToF | Op::FNeg | Op::FAdd | Op::FSub | Op::FMul
        )
}

/// Refuses a new zero-trip edge when it would bypass SSA definitions observed after the loop.
fn header_values_escape(function: &Function, loop_ref: &NaturalLoop) -> bool {
    let header = &function.blocks[loop_ref.header.as_raw() as usize];
    let mut values: HashSet<_> = header.params.iter().copied().collect();
    values.extend(header.instructions.iter().filter_map(|id| {
        function.instructions[id.as_raw() as usize].result
    }));
    function.blocks.iter().filter(|block| !loop_ref.contains(block.id)).any(|block| {
        block.instructions.iter().any(|id| {
            function.instructions[id.as_raw() as usize].operands.iter().any(|value| values.contains(value))
        }) || block.terminator.as_ref().is_some_and(|term| {
            term.used_values().iter().any(|value| values.contains(value))
        })
    })
}

/// Clones the first scalar condition into the preheader and inserts a taken-only property block.
fn guard_entry(
    function: &mut Function, loop_ref: &NaturalLoop, preheader: BlockId, body: BlockId,
) -> Option<BlockId> {
    let header = function.blocks[loop_ref.header.as_raw() as usize].clone();
    let Terminator::Br { target, args } = function.block(preheader)?.terminator.clone()? else { return None; };
    if target != header.id || args.len() != header.params.len() { return None; }
    let Terminator::CondBr { cond, then_target, then_args, else_target, else_args } =
        header.terminator? else { return None; };
    let mut replacements: HashMap<_, _> = header.params.iter().copied().zip(args.iter().copied()).collect();
    // Validate the clone's inputs before mutating the function.
    let dominance = compute_dominance(function);
    let mut available: HashSet<_> = header.params.iter().copied().collect();
    for &id in &header.instructions {
        let inst = &function.instructions[id.as_raw() as usize];
        if !inst.operands.iter().all(|value| {
            available.contains(value) || function.value(*value).is_some_and(|value| {
                let block = match value.def {
                    ValueDef::Instruction { block, .. } | ValueDef::BlockParam { block, .. } => block,
                };
                !loop_ref.contains(block) && dominance.dominates(block, preheader)
            })
        }) { return None; }
        available.extend(inst.result);
    }
    for id in header.instructions {
        let mut inst = function.instructions[id.as_raw() as usize].clone();
        for operand in &mut inst.operands {
            *operand = replacements.get(operand).copied().unwrap_or(*operand);
        }
        let original = inst.result;
        let result = append_clone(function, preheader, inst);
        if let (Some(original), Some(result)) = (original, result) {
            replacements.insert(original, result);
        }
    }
    let remap = |value: ValueId| replacements.get(&value).copied().unwrap_or(value);
    let guarded = BlockId::from_raw(function.blocks.len() as u32);
    let mut block = BasicBlock::new(guarded, format!("buffer_property_entry_{}", guarded.as_raw()), vec![]);
    block.terminator = Some(Terminator::Br { target: header.id, args });
    function.blocks.push(block);
    let then_args = then_args.into_iter().map(remap).collect();
    let else_args = else_args.into_iter().map(remap).collect();
    function.blocks[preheader.as_raw() as usize].terminator = Some(Terminator::CondBr {
        cond: remap(cond),
        then_target: if then_target == body { guarded } else { then_target },
        then_args: if then_target == body { vec![] } else { then_args },
        else_target: if else_target == body { guarded } else { else_target },
        else_args: if else_target == body { vec![] } else { else_args },
    });
    Some(guarded)
}

/// Relocates a read without changing its id and clones its exact `$this` load into the preheader.
fn move_read(function: &mut Function, source: BlockId, read: InstId, target: BlockId) {
    let receiver = function.instructions[read.as_raw() as usize].operands[0];
    let ValueDef::Instruction { inst, .. } = function.values[receiver.as_raw() as usize].def
        else { unreachable!("trusted property receivers are local loads"); };
    let load = function.instructions[inst.as_raw() as usize].clone();
    let receiver = append_clone(function, target, load).expect("receiver load has a result");
    function.instructions[read.as_raw() as usize].operands[0] = receiver;
    function.instructions[read.as_raw() as usize].origin = Some(PassOrigin::BufferAccess);
    function.blocks[source.as_raw() as usize].instructions.retain(|id| *id != read);
    function.blocks[target.as_raw() as usize].instructions.push(read);
    for block in &function.blocks {
        for (index, &id) in block.instructions.iter().enumerate() {
            if let Some(result) = function.instructions[id.as_raw() as usize].result {
                function.values[result.as_raw() as usize].def = ValueDef::Instruction {
                    block: block.id, index: index as u32, inst: id,
                };
            }
        }
    }
}

/// Appends an exact instruction clone with a fresh result value and updated definition position.
fn append_clone(function: &mut Function, block: BlockId, mut inst: Instruction) -> Option<ValueId> {
    let id = InstId::from_raw(function.instructions.len() as u32);
    let result = inst.result.map(|original| {
        let result = ValueId::from_raw(function.values.len() as u32);
        let original = function.values[original.as_raw() as usize].clone();
        function.values.push(Value {
            def: ValueDef::Instruction {
                block, index: function.blocks[block.as_raw() as usize].instructions.len() as u32, inst: id,
            },
            ..original
        });
        result
    });
    inst.result = result;
    inst.origin = Some(PassOrigin::BufferAccess);
    function.instructions.push(inst);
    function.blocks[block.as_raw() as usize].instructions.push(id);
    result
}
