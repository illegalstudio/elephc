//! Purpose:
//! Fuses integer-cast numeric expressions through private reaching local writers.
//!
//! Called from:
//! - CheckedNumericChain before its direct-expression phase.
//!
//! Key details:
//! - Each selected store must be the unique reaching definition at every rewritten read.
//! - Each cast evaluates the original ordered float-promoting chain, including its suffix.

use super::*;
use crate::ir::{LocalKind, LocalSlotId};

#[derive(Default)]
struct Region {
    instructions: HashSet<InstId>,
    slots: HashSet<LocalSlotId>,
    sinks: HashMap<InstId, ChainRewrite>,
    arithmetic: HashSet<InstId>,
    writers: HashSet<InstId>,
}

/// Collects connected numeric graphs and removes their storage only after all observers pass.
pub(super) fn run(function: &mut Function) -> bool {
    if function.instructions.len() > 4096 || crate::ir_passes::cfg::has_exception_handlers(function)
        || crate::ir_passes::local_scope::has_dynamic_eval(function)
        || function.flags.is_generator || function.flags.is_fiber_wrapper
        || function.locals.iter().any(|local| matches!(local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope | LocalKind::GeneratorState)) {
        return false;
    }
    let candidates: Vec<_> = function.instructions.iter().enumerate().filter_map(|(raw, inst)| {
        integer_cast(inst).then_some(InstId::from_raw(raw as u32))
    }).take(64).collect();
    let mut changed = false;
    for sink in candidates {
        let uses = instruction_uses(function);
        let terms = terminator_used_values(function);
        let mut region = Region::default();
        if !gather(function, sink, &uses, &terms, &mut region) || region.slots.is_empty() { continue; }
        for (sink, rewrite) in region.sinks { apply_rewrite(function, sink, rewrite); }
        for id in region.instructions { neutralize_to_nop(&mut function.instructions[id.as_raw() as usize]); }
        for slot in region.slots {
            // A cloned fast path can eliminate its writer while the generic copy still
            // needs boxed storage. Keep its retirement and frame contract in that case.
            if function.instructions.iter().any(|inst| {
                super::super::checked_int_sink::instruction_mentions_slot(inst, slot)
                    && inst.op != Op::ReleaseLocalSlot
            }) { continue; }
            for inst in &mut function.instructions {
                if inst.immediate == Some(Immediate::LocalSlot(slot)) {
                    neutralize_to_nop(inst);
                }
            }
            let local = &mut function.locals[slot.as_raw() as usize];
            local.ir_type = IrType::I64; local.php_type = PhpType::Int;
        }
        changed = true;
    }
    changed
}

/// Accepts only explicit PHP integer casts, never typed stores or return coercions.
fn integer_cast(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::Cast && inst.result_php_type == PhpType::Int
        && inst.immediate == Some(Immediate::CastTarget(IrType::I64))
}

/// Grows a graph through every slot load and every arithmetic observer to its cast sinks.
fn gather(function: &Function, first: InstId, uses: &HashMap<ValueId, Vec<InstId>>, terms: &HashSet<ValueId>, region: &mut Region) -> bool {
    let mut pending = vec![first];
    let mut examined = HashSet::new();
    while let Some(id) = pending.pop() {
        if region.instructions.len() > 256 || region.slots.len() > 16 { return false; }
        if !examined.insert(id) { continue; }
        let Some(inst) = function.instruction(id) else { return false; };
        if integer_cast(inst) {
            let Some(&root) = inst.operands.first() else { return false; };
            let Some(rewrite) = expression(function, root, region, &mut HashSet::new()) else { return false; };
            if rewrite.operations.is_empty() { return false; }
            region.sinks.insert(id, rewrite);
        } else {
            // Every downstream arithmetic observation must eventually reach a cast. Its complete
            // expression is assembled there, while the forward walk discovers all such sinks.
            region.instructions.insert(id);
        }
        let snapshot: Vec<_> = region.instructions.iter().copied().collect();
        for member in snapshot {
            let Some(member) = function.instruction(member) else { return false; };
            let Some(value) = member.result else { continue; };
            if terms.contains(&value) { return false; }
            for &user in uses.get(&value).into_iter().flatten() {
                if region.instructions.contains(&user) || region.sinks.contains_key(&user) { continue; }
                let Some(inst) = function.instruction(user) else { return false; };
                match inst.op {
                    Op::Release | Op::Acquire | Op::MixedClone | Op::Move | Op::Borrow if inst.immediate.is_none() => pending.push(user),
                    Op::MixedNumericBinop if matches!(inst.immediate, Some(Immediate::MixedNumericOp(MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))) => pending.push(user),
                    Op::Cast if integer_cast(inst) => pending.push(user),
                    Op::StoreLocal if region.writers.contains(&user) => pending.push(user),
                    _ => return false,
                }
            }
        }
    }
    // A dangling non-cast branch would otherwise turn an observable boxed result into a Nop.
    region.instructions.iter().all(|id| {
        function.instruction(*id).is_some_and(|inst| {
            if matches!(inst.op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul | Op::MixedNumericBinop)
                && !region.arithmetic.contains(id) { return false; }
            inst.result.is_none_or(|value| {
            uses.get(&value).is_none_or(|users| users.iter().all(|id| region.instructions.contains(id) || region.sinks.contains_key(id)))
        }) })
    })
}

/// Reconstructs a left-associated chain through transparent owners and dominated local reads.
fn expression(function: &Function, value: ValueId, region: &mut Region, visiting: &mut HashSet<ValueId>) -> Option<ChainRewrite> {
    if !visiting.insert(value) || visiting.len() > 64 { return None; }
    let (id, inst) = defining_instruction(function, value)?;
    let mut rewrite = match inst.op {
        Op::Acquire | Op::MixedClone | Op::Move | Op::Borrow if inst.immediate.is_none() => {
            expression(function, *inst.operands.first()?, region, visiting)?
        }
        Op::LoadLocal => {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
            let local = function.locals.get(slot.as_raw() as usize)?;
            if local.kind != LocalKind::PhpLocal || (slot.as_raw() as usize) < function.params.len()
                || local.php_type != PhpType::Mixed || function.no_epilogue_cleanup_slots.contains(&slot)
                || crate::ir_passes::address_escaping_slots(function).contains(&slot) { return None; }
            let dominance = crate::ir_passes::compute_dominance(function);
            let mut writer = None;
            for (raw, access) in function.instructions.iter().enumerate() {
                if !super::super::checked_int_sink::instruction_mentions_slot(access, slot) { continue; }
                let access_id = InstId::from_raw(raw as u32);
                match access.op {
                    Op::StoreLocal if dominates_access(function, &dominance, access_id, id) => {
                        if writer.is_none_or(|previous| dominates_access(function, &dominance, previous, access_id)) {
                            writer = Some(access_id);
                        }
                    }
                    Op::StoreLocal | Op::LoadLocal | Op::ReleaseLocalSlot => {}
                    _ => return None,
                }
            }
            let writer = writer?;
            let loads = reaching_reads(function, writer, slot)?;
            let predecessors = crate::ir_passes::cfg::predecessors(function);
            if !loads.contains(&id) || loads.iter().any(|&load| {
                !dominates_access(function, &dominance, writer, load)
                    || !unique_reaching_writer(function, &predecessors, writer, load, slot)
            }) {
                return None;
            }
            region.instructions.extend(loads);
            region.slots.insert(slot); region.instructions.insert(writer); region.writers.insert(writer);
            let source = *function.instruction(writer)?.operands.first()?;
            expression(function, source, region, visiting)?
        }
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul => {
            let &[lhs, rhs] = inst.operands.as_slice() else { return None; };
            if !integer_value(function, lhs) || !integer_value(function, rhs) { return None; }
            ChainRewrite { operands: vec![lhs, rhs], operations: vec![checked_operation(inst.op)?], neutralize: HashSet::new() }
        }
        Op::MixedNumericBinop => {
            let &[lhs, rhs] = inst.operands.as_slice() else { return None; };
            let Some(Immediate::MixedNumericOp(op @ (MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))) = inst.immediate else { return None; };
            if !integer_value(function, rhs) { return None; }
            if integer_value(function, lhs) {
                ChainRewrite { operands: vec![lhs, rhs], operations: vec![op], neutralize: HashSet::new() }
            } else {
                let mut rewrite = expression(function, lhs, region, visiting)?;
                rewrite.operands.push(rhs); rewrite.operations.push(op); rewrite
            }
        }
        _ => return None,
    };
    region.instructions.insert(id);
    if matches!(inst.op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul | Op::MixedNumericBinop) {
        region.arithmetic.insert(id);
    }
    rewrite.neutralize.clear();
    visiting.remove(&value);
    Some(rewrite)
}

/// Checks instruction dominance, including source order for accesses in the same block.
fn dominates_access(function: &Function, dominance: &crate::ir_passes::DominanceInfo, first: InstId, second: InstId) -> bool {
    let Some(first_block) = instruction_block(function, first) else { return false; };
    let Some(second_block) = instruction_block(function, second) else { return false; };
    if first_block != second_block { return dominance.dominates(first_block, second_block); }
    let instructions = &function.blocks[first_block.as_raw() as usize].instructions;
    instructions.iter().position(|id| *id == first).zip(instructions.iter().position(|id| *id == second))
        .is_some_and(|(first, second)| first < second)
}

/// Finds every observation of one store before a later overwrite or explicit retirement.
fn reaching_reads(function: &Function, writer: InstId, slot: LocalSlotId) -> Option<HashSet<InstId>> {
    let start = instruction_block(function, writer)?;
    let position = function.blocks[start.as_raw() as usize].instructions.iter().position(|id| *id == writer)?;
    let mut pending = vec![(start, position + 1)];
    let mut seen = HashSet::new();
    let mut loads = HashSet::new();
    while let Some((block, position)) = pending.pop() {
        if !seen.insert((block, position)) { continue; }
        let block = function.block(block)?;
        let mut killed = false;
        for &id in &block.instructions[position..] {
            let inst = function.instruction(id)?;
            if inst.immediate != Some(Immediate::LocalSlot(slot)) { continue; }
            match inst.op {
                Op::LoadLocal => { loads.insert(id); }
                Op::StoreLocal | Op::ReleaseLocalSlot => { killed = true; break; }
                _ => return None,
            }
        }
        if !killed {
            pending.extend(block.terminator.as_ref().map(crate::ir_passes::cfg::successors)
                .unwrap_or_default().into_iter().map(|block| (block, 0)));
        }
    }
    Some(loads)
}

/// Rejects a read when any incoming path reaches a different store, retirement or uninitialized entry.
fn unique_reaching_writer(
    function: &Function, predecessors: &[Vec<BlockId>], writer: InstId, load: InstId, slot: LocalSlotId,
) -> bool {
    let Some(start) = instruction_block(function, load) else { return false; };
    let Some(position) = function.blocks[start.as_raw() as usize].instructions.iter().position(|id| *id == load)
    else { return false; };
    let mut pending = vec![(start, position)];
    let mut seen = HashSet::new();
    while let Some((block, position)) = pending.pop() {
        if !seen.insert((block, position)) { continue; }
        let block = &function.blocks[block.as_raw() as usize];
        let mut found = false;
        for &id in block.instructions[..position].iter().rev() {
            let inst = &function.instructions[id.as_raw() as usize];
            if inst.immediate != Some(Immediate::LocalSlot(slot)) { continue; }
            match inst.op {
                Op::StoreLocal if id == writer => { found = true; break; }
                Op::LoadLocal => {}
                _ => return false,
            }
        }
        if found { continue; }
        if block.id == function.entry || predecessors[block.id.as_raw() as usize].is_empty() {
            return false;
        }
        pending.extend(predecessors[block.id.as_raw() as usize].iter().map(|&previous| {
            (previous, function.blocks[previous.as_raw() as usize].instructions.len())
        }));
    }
    true
}

/// Requires PHP integer metadata as well as an integer-width payload.
fn integer_value(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|value| value.ir_type == IrType::I64 && value.php_type == PhpType::Int)
}
