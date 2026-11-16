//! Purpose:
//! Skips counter loops whose checked seed overflow makes their first condition false.
//!
//! Called from:
//! - Counter versioning before allocating any cloned loop blocks.
//!
//! Key details:
//! - The counter must be dead outside its loop and the skipped initializer suffix numeric.
//! - Only directional constant offsets and exact integer comparisons are accepted.

use super::*;

/// Exhausts dead-at-exit seed rewrites before the caller spends its loop-copy budget.
pub(super) fn run(function: &mut Function, data: &DataPool, pass: &BoundedMixedCounter) -> bool {
    let mut changed = false;
    loop {
        let dominance = crate::ir_passes::compute_dominance(function);
        let loops = crate::ir_passes::loops::compute_loops(function, &dominance);
        let trial = loops.loops().iter().filter(|lp| {
            !loops.loops().iter().any(|inner| inner.header != lp.header && lp.contains(inner.header))
        }).find_map(|lp| {
            let trial = candidate(function, lp, data, pass)?;
            crate::ir::validate_function(&trial).is_ok().then_some(trial)
        });
        let Some(trial) = trial else { return changed; };
        // A committed rewrite removes a Mixed recurrence, so it cannot be selected again.
        *function = trial;
        changed = true;
    }
}

/// Builds a private guarded seed rewrite, declining every observable overflow result.
fn candidate(function: &Function, lp: &NaturalLoop, data: &DataPool, pass: &BoundedMixedCounter) -> Option<Function> {
    let preheader = lp.preheader?;
    let header = function.block(lp.header)?;
    let Terminator::CondBr { cond, then_target, else_target, .. } = header.terminator.as_ref()? else { return None; };
    if !lp.contains(*then_target) || lp.contains(*else_target) || !function.block(*else_target)?.params.is_empty() { return None; }
    let condition = defining_instruction(function, *cond)?;
    if condition.op != Op::PhpRelCmp || !header.instructions.contains(&defining_id(function, *cond)?)
        || header.instructions.iter().any(|id| {
        !matches!(function.instructions[id.as_raw() as usize].op,
            Op::LoadLocal | Op::ConstI64 | Op::PhpRelCmp | Op::Release | Op::Move | Op::Borrow)
    }) { return None; }
    let &[counter, bound] = condition.operands.as_slice() else { return None; };
    if !integer(function, bound) { return None; }
    let slot = counter_slot(function, counter)?;
    if !header.instructions.contains(&defining_id(function, counter)?) { return None; }
    let local = function.locals.get(slot.as_raw() as usize)?;
    if local.kind != LocalKind::PhpLocal || (slot.as_raw() as usize) < function.params.len()
        || local.php_type != PhpType::Mixed || function.no_epilogue_cleanup_slots.contains(&slot) { return None; }
    let Some(Immediate::CmpPredicate(predicate)) = condition.immediate else { return None; };
    let mut store = None;
    let mut updates = Vec::new();
    for block in &function.blocks {
        if lp.contains(block.id) {
            updates.extend(block.instructions.iter().filter_map(|id| {
                let inst = function.instruction(*id)?;
                (inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot))).then_some(*id)
            }));
            continue;
        }
        for &id in &block.instructions {
            let inst = function.instruction(id)?;
            if inst.immediate != Some(Immediate::LocalSlot(slot)) { continue; }
            match inst.op {
                Op::StoreLocal if block.id == preheader && store.is_none() => store = Some(id),
                Op::ReleaseLocalSlot => {}
                _ => return None,
            }
        }
    }
    let stored = *function.instruction(store?)?.operands.first()?;
    let producer = storage::owner_source(function, stored)?;
    let producer_id = defining_id(function, producer.result?)?;
    let &[base, offset] = producer.operands.as_slice() else { return None; };
    if !integer(function, base) { return None; }
    let literal = defining_instruction(function, offset)?;
    let Some(Immediate::I64(offset)) = literal.immediate else { return None; };
    if literal.op != Op::ConstI64 || offset <= 0 { return None; }
    let (threshold, guard, arithmetic, step) = match (producer.op, predicate) {
        (Op::ICheckedSub, CmpPredicate::Sgt) => (i64::MIN.checked_add(offset)?, CmpPredicate::Sge, Op::ISub, -1),
        (Op::ICheckedSub, CmpPredicate::Sge) if zero(function, bound) => (i64::MIN.checked_add(offset)?, CmpPredicate::Sge, Op::ISub, -1),
        (Op::ICheckedAdd, CmpPredicate::Slt) => (i64::MAX.checked_sub(offset)?, CmpPredicate::Sle, Op::IAdd, 1),
        (Op::ICheckedAdd, CmpPredicate::Sle) if zero(function, bound) => (i64::MAX.checked_sub(offset)?, CmpPredicate::Sle, Op::IAdd, 1),
        _ => return None,
    };
    let proof = committed_proof(function, slot, pass, data)?;
    if proof.updates.len() != 1 || !proof.seeds.contains(&producer_id) { return None; }
    let update = function.instruction(*proof.updates.iter().next()?)?;
    if update.immediate != Some(Immediate::I64(step)) || updates.len() != 1
        || storage::owner_source(function, *function.instruction(updates[0])?.operands.first()?)?.result != update.result {
        return None;
    }
    if let Some(bound_load) = defining_instruction(function, bound) {
        if let (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) = (bound_load.op, &bound_load.immediate) {
            if function.locals[slot.as_raw() as usize].ir_type != IrType::I64 { return None; }
        }
    }
    let block = function.block(preheader)?;
    let split = block.instructions.iter().position(|id| *id == producer_id)?;
    // Skipping may omit numeric bookkeeping only. Other initialization effects must still run.
    for &id in &block.instructions[split..] {
        let inst = function.instruction(id)?;
        if id == producer_id || proof.releases.contains(&id) { continue; }
        if matches!(inst.op, Op::Nop | Op::ConstI64 | Op::ConstBool) { continue; }
        if inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot)) { continue; }
        if matches!(inst.op, Op::Acquire | Op::MixedClone | Op::Borrow | Op::Move)
            && inst.result.is_some_and(|value| proof.scalar_values.contains(&value)) { continue; }
        return None;
    }
    let mut trial = function.clone();
    proof.scalarize(&mut trial, slot);
    trial.instructions[producer_id.as_raw() as usize].op = arithmetic;
    trial.instructions[producer_id.as_raw() as usize].effects = arithmetic.default_effects();
    for id in &proof.updates {
        let inst = &mut trial.instructions[id.as_raw() as usize];
        inst.op = Op::IAdd; inst.effects = Op::IAdd.default_effects();
    }
    let split = trial.blocks[preheader.as_raw() as usize].instructions.iter().position(|id| *id == producer_id)?;
    let suffix = trial.blocks[preheader.as_raw() as usize].instructions.split_off(split);
    let terminator = trial.blocks[preheader.as_raw() as usize].terminator.take()?;
    let mut builder = Builder::new(&mut trial);
    let seed_block = builder.create_named_block("counter.seed.inrange", vec![]);
    builder.position_at_end(preheader);
    let threshold = builder.emit_const_i64(threshold);
    let safe = emit(&mut builder, Op::ICmp, vec![base, threshold], Some(Immediate::CmpPredicate(guard)), IrType::I64, PhpType::Bool);
    builder.terminate(Terminator::CondBr { cond: safe, then_target: seed_block, then_args: vec![], else_target: *else_target, else_args: vec![] });
    drop(builder);
    for (index, &id) in suffix.iter().enumerate() {
        if let Some(value) = trial.instructions[id.as_raw() as usize].result {
            trial.values[value.as_raw() as usize].def = ValueDef::Instruction { block: seed_block, index: index as u32, inst: id };
        }
    }
    trial.blocks[seed_block.as_raw() as usize].instructions = suffix;
    trial.blocks[seed_block.as_raw() as usize].terminator = Some(terminator);
    Some(trial)
}

/// Requires an exact zero, safely separated from either overflowed floating-point boundary.
fn zero(function: &Function, value: ValueId) -> bool {
    defining_instruction(function, value).is_some_and(|inst| inst.op == Op::ConstI64 && inst.immediate == Some(Immediate::I64(0)))
}
