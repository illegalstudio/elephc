//! Purpose:
//! Reuses integer-range proofs to retain scalar storage for nonoverflowing loop counters.
//!
//! Called from:
//! - Numeric storage replay after every potentially promoting local has been widened.
//!
//! Key details:
//! - Trials are discarded; successful proofs only authorize another ordinary AST lowering.
//! - By-value call positions are supplied by authoritative lowering signatures.
//! - Unknown aliases, handlers, eval, undefined reads and unproven updates fail closed.
//! - A slot whose checked seed reads another counter is retried jointly with that counter, and
//!   the proven incrementing slots that needed such a seed are reported separately, because
//!   lowering has to type that seed as an integer before the slot can read as one.

use super::*;

/// Integer-proven counter slots, plus those whose proof also needed a checked seed.
#[derive(Default)]
pub(crate) struct ProvenSlots {
    /// Every slot whose recurrence, seeds included, was proven to stay integer.
    pub(crate) slots: HashSet<LocalSlotId>,
    /// The proven slots an incrementing counter seeds with checked arithmetic (`$j = $i + 1`).
    /// Their seed only reads as the integer it always is once lowering is told so; the update
    /// alone already widens the slot to the boxed storage the replay starts from.
    pub(crate) seeded: HashSet<LocalSlotId>,
}

/// Finds counters whose recurrence and comparison observers permit integer storage,
/// and reports which proofs also needed a checked seed.
pub(crate) fn prove_integer_slots(
    function: &Function,
    data: &mut DataPool,
    readonly_calls: &HashMap<InstId, HashSet<usize>>,
) -> ProvenSlots {
    let mut result = ProvenSlots::default();
    if function.instructions.len() > 4096
        || function.flags.is_generator || function.flags.is_fiber_wrapper
        || super::super::cfg::has_exception_handlers(function)
        || super::super::local_scope::has_dynamic_eval(function)
        || function.locals.iter().any(|local| matches!(local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope | LocalKind::GeneratorState))
    { return result; }
    let candidates: Vec<_> = function.locals.iter().filter(|local| {
        local.kind == LocalKind::PhpLocal && local.php_type == PhpType::Mixed
            && local.ir_type == IrType::Heap(IrHeapKind::Mixed)
            && local.id.as_raw() as usize >= function.params.len()
            && !function.no_epilogue_cleanup_slots.contains(&local.id)
    }).map(|local| local.id).collect();
    let incrementing: HashSet<_> = candidates.iter().copied().filter(|&slot| {
        function.instructions.iter().any(|inst| {
            inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
                && inst
                    .operands
                    .first()
                    .is_some_and(|&value| increment_writer(function, slot, value))
        })
    }).collect();
    // Mutually observed counters cannot assume each other's tags independently.
    // A joint trial proves all updates before retaining any of their scalar slots.
    let group: HashSet<_> = candidates.iter().copied()
        .filter(|slot| incrementing.contains(slot)).take(16).collect();
    if group.len() > 1 {
        if let Some(proof) = CounterProof::for_slots(
            function, &group, &BoundedMixedCounter::default(), data, Some(readonly_calls),
        ) {
            if trial_safe(function, &group, &proof, data) {
                result.record(&group, &proof, &incrementing);
                return result;
            }
        }
    }
    let mut trials = 0;
    for &slot in &candidates {
        let Some(proof) = CounterProof::with_readonly_calls(
            function, slot, &BoundedMixedCounter::default(), data, Some(readonly_calls),
        ) else { continue; };
        if trials == 16 { break; }
        trials += 1;
        let slots = HashSet::from([slot]);
        if trial_safe(function, &slots, &proof, data) {
            result.record(&slots, &proof, &incrementing);
        }
    }
    // A seed that reads another counter (`$j = $i + 1` in the body of the `$i` loop) is provable
    // only together with that counter. Alone, the other counter's boxed reads carry no range;
    // and a counter proven in an earlier round keeps boxed stores until the optimizer narrows
    // them, so mem2reg cannot promote it inside a later trial either.
    let mut joint_trials = 0;
    for &slot in &candidates {
        if joint_trials == 8 { break; }
        if result.slots.contains(&slot) { continue; }
        let mut slots = seed_dependencies(function, slot, &candidates);
        if slots.is_empty() { continue; }
        slots.insert(slot);
        let Some(proof) = CounterProof::for_slots(
            function, &slots, &BoundedMixedCounter::default(), data, Some(readonly_calls),
        ) else { continue; };
        joint_trials += 1;
        if trial_safe(function, &slots, &proof, data) {
            result.record(&slots, &proof, &incrementing);
        }
    }
    result
}

impl ProvenSlots {
    /// Adds one successful trial: all of its slots, and the incrementing ones it seeded.
    fn record(
        &mut self,
        slots: &HashSet<LocalSlotId>,
        proof: &CounterProof,
        incrementing: &HashSet<LocalSlotId>,
    ) {
        self.slots.extend(slots.iter().copied());
        self.seeded.extend(proof.seeded_slots.iter().copied()
            .filter(|slot| slots.contains(slot) && incrementing.contains(slot)));
    }
}

/// Returns the other candidate counters whose values a checked seed of `slot` reads.
fn seed_dependencies(
    function: &Function,
    slot: LocalSlotId,
    candidates: &[LocalSlotId],
) -> HashSet<LocalSlotId> {
    let mut dependencies = HashSet::new();
    for inst in &function.instructions {
        if inst.op != Op::StoreLocal || inst.immediate != Some(Immediate::LocalSlot(slot)) {
            continue;
        }
        let Some(producer) = inst.operands.first().and_then(|&value| owner_source(function, value))
        else { continue; };
        if seed_op(producer).is_none() { continue; }
        for &operand in &producer.operands {
            let Some(load) = owner_source(function, operand) else { continue; };
            if let (Op::LoadLocal, Some(Immediate::LocalSlot(source))) =
                (load.op, load.immediate.as_ref())
            {
                if *source != slot && candidates.contains(source) {
                    dependencies.insert(*source);
                }
            }
        }
    }
    dependencies
}

/// Follows acquire, clone, move and borrow wrappers back to the instruction that made a value.
pub(super) fn owner_source(function: &Function, mut value: ValueId) -> Option<&Instruction> {
    for _ in 0..8 {
        let inst = super::defining_instruction(function, value)?;
        match inst.op {
            Op::Acquire | Op::MixedClone | Op::Move | Op::Borrow | Op::EnsureOwned => {
                value = *inst.operands.first()?;
            }
            _ => return Some(inst),
        }
    }
    None
}

/// Follows ownership wrappers only; another local's increment is not this slot's recurrence.
///
/// Both PHP increment forms count: `$i++`/`$i--` (`str_inc_dec`) and the compound
/// `$i += 1`/`$i -= 1` shape, which lowers to checked or mixed `+`/`-` with a constant step.
fn increment_writer(function: &Function, slot: LocalSlotId, mut value: ValueId) -> bool {
    for _ in 0..function.values.len() {
        let Some(inst) = super::defining_instruction(function, value) else { return false; };
        match inst.op {
            Op::StrIncDec => return true,
            Op::Acquire | Op::MixedClone | Op::Move | Op::Borrow | Op::EnsureOwned => {
                let Some(&source) = inst.operands.first() else { return false; };
                value = source;
            }
            _ => return super::counter_step(function, slot, inst),
        }
    }
    false
}

/// Proves every update on a discarded scalar SSA trial, including nested-loop invariants.
fn trial_safe(function: &Function, slots: &HashSet<LocalSlotId>, proof: &CounterProof, data: &mut DataPool) -> bool {
    let mut trial = function.clone();
    // Discovery runs before finalization. An unfinished exit is a sink, never a backedge.
    for block in &mut trial.blocks {
        if block.terminator.is_none() {
            block.terminator = Some(crate::ir::Terminator::Unreachable);
        }
    }
    proof.scalarize_slots(&mut trial, slots);
    if crate::ir::validate_function(&trial).is_err() { return false; }
    super::super::mem2reg::Mem2Reg.run(&mut trial, data);
    if trial.instructions.iter().any(|inst| {
        matches!(inst.op, Op::LoadLocal | Op::StoreLocal)
            && matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if slots.contains(&slot))
    }) { return false; }
    for _ in 0..16 {
        let changed = fold_trivial_parameters(&mut trial);
        if !super::super::loop_optimize::LoopOptimize.run(&mut trial, data) && !changed { break; }
    }
    super::super::integer_range::IntegerRange.run(&mut trial, data);
    proof.proven_unchecked(&trial) && crate::ir::validate_function(&trial).is_ok()
}

/// Removes unchanged scalar joins only on the discarded trial before induction recognition.
fn fold_trivial_parameters(function: &mut Function) -> bool {
    use crate::ir::BlockId;
    let dominance = super::super::compute_dominance(function);
    let mut incoming: HashMap<BlockId, Vec<Vec<ValueId>>> = HashMap::new();
    for block in &function.blocks {
        if let Some(mut term) = block.terminator.clone() {
            for (target, args) in super::super::cfg::successor_edges(&mut term) {
                incoming.entry(*target).or_default().push(args.clone());
            }
        }
    }
    let mut replacements = HashMap::new();
    let mut removed = Vec::new();
    for block in &function.blocks {
        if block.id == function.entry || !dominance.is_reachable(block.id) { continue; }
        let Some(edges) = incoming.get(&block.id) else { continue; };
        let mut positions = Vec::new();
        for (index, &parameter) in block.params.iter().enumerate() {
            let Some(metadata) = function.value(parameter) else { continue; };
            if metadata.ir_type != IrType::I64 || metadata.php_type != PhpType::Int
                || metadata.ownership != Ownership::NonHeap
                || edges.iter().any(|args| args.get(index).is_none())
            { continue; }
            let mut sources = edges.iter().map(|args| args[index]).filter(|&value| value != parameter);
            let Some(source) = sources.next() else { continue; };
            if sources.any(|value| value != source) { continue; }
            let Some(value) = function.value(source) else { continue; };
            if value.ir_type != metadata.ir_type || value.php_type != metadata.php_type
                || value.ownership != metadata.ownership
            { continue; }
            let source_block = match value.def {
                ValueDef::Instruction { block, .. } | ValueDef::BlockParam { block, .. } => block,
            };
            // A replacement must be available before this join. A seedless parameter
            // cycle or a same-block backedge result cannot supply an entry value.
            if !dominance.strictly_dominates(source_block, block.id) { continue; }
            replacements.insert(parameter, source);
            positions.push(index);
        }
        if !positions.is_empty() { removed.push((block.id, positions)); }
    }
    if replacements.is_empty() { return false; }
    super::replace_all_uses(function, &super::resolve_chains(&replacements));
    for (block, positions) in removed {
        super::super::loop_cfg::remove_parameters(function, block, &positions);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, Terminator};

    /// Builds two parallel conditional edges carrying equal or different scalar join values.
    fn parallel_join(same: bool) -> Function {
        let mut function = Function::new("parallel_join".into(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let join = builder.create_named_block("join", vec![(IrType::I64, PhpType::Int)]);
        let result = builder.block_param(join, 0);
        builder.set_entry(entry); builder.position_at_end(entry);
        let left = builder.emit_const_i64(7);
        let right = if same { left } else { builder.emit_const_i64(9) };
        let condition = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr { cond: condition,
            then_target: join, then_args: vec![left], else_target: join, else_args: vec![right] });
        builder.position_at_end(join);
        builder.terminate(Terminator::Return { value: Some(result) });
        crate::ir::validate_function(&function).unwrap();
        function
    }

    /// A common dominating input may replace a scalar phi only when every parallel edge agrees.
    #[test]
    fn method_counter_trial_join_checks_every_parallel_edge() {
        for same in [false, true] {
            let mut function = parallel_join(same);
            let before = crate::ir::print_function(&function);
            assert_eq!(fold_trivial_parameters(&mut function), same);
            crate::ir::validate_function(&function).unwrap();
            if same { assert!(function.blocks[1].params.is_empty()); }
            else { assert_eq!(crate::ir::print_function(&function), before); }
        }
    }

    /// Equal payload words do not authorize replacing a PHP boolean phi as an integer counter.
    #[test]
    fn method_counter_trial_join_preserves_noninteger_tags() {
        let mut function = parallel_join(true);
        function.return_php_type = PhpType::Bool;
        for metadata in &mut function.values {
            if metadata.php_type == PhpType::Int { metadata.php_type = PhpType::Bool; }
        }
        for inst in &mut function.instructions {
            if inst.result_php_type == PhpType::Int { inst.result_php_type = PhpType::Bool; }
            if inst.op == Op::ConstI64 {
                inst.op = Op::ConstBool; inst.immediate = Some(Immediate::Bool(true));
            }
        }
        crate::ir::validate_function(&function).unwrap();
        assert!(!fold_trivial_parameters(&mut function));
        assert_eq!(function.blocks[1].params.len(), 1);
    }
}
