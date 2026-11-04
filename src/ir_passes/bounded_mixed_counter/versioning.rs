//! Purpose:
//! Adds guarded integer copies of canonical boxed counter loops.
//!
//! Called from:
//! - The fixed-point driver after static bounded-counter proofs.
//!
//! Key details:
//! - The original EIR loop remains the fallback for float seeds and unsafe inclusive bounds.
//! - Only EIR blocks are copied, so AST binding decisions and declarations execute once.

use super::*;
use crate::ir::{BlockId, Builder, CmpPredicate, Instruction, Terminator};
use crate::ir_passes::loops::NaturalLoop;

mod clone;
mod seed;

/// Adds guarded integer paths after the ordinary static proof has declined a counter.
pub(in crate::ir_passes) struct LoopCounterVersioning {
    pass: BoundedMixedCounter,
}

impl LoopCounterVersioning {
    /// Retains authoritative callee ABI facts shared with the static counter proof.
    pub(in crate::ir_passes) fn new(module: &Module) -> Self {
        Self { pass: BoundedMixedCounter::new(module) }
    }
}

impl IrPass for LoopCounterVersioning {
    /// Names guarded specialization separately from unconditional whole-slot scalarization.
    fn name(&self) -> &'static str { "loop_counter_versioning" }

    /// Avoids CFG analysis when no boxed step-one update remains.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| inst.op == Op::StrIncDec)
    }

    /// Commits only fully validated guarded rewrites, retaining their generic fallback.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        run(function, data, &self.pass)
    }
}

/// Bounds code growth and rejects scopes whose storage can be reached implicitly.
fn run(function: &mut Function, data: &DataPool, pass: &BoundedMixedCounter) -> bool {
    if (function.flags.is_main && !super::main_frame_is_private(function, data))
        || function.instructions.len() > 2048
        || function.locals.len() > 128
        || function.flags.is_generator || function.flags.is_fiber_wrapper
        || crate::ir_passes::cfg::has_exception_handlers(function)
        || crate::ir_passes::local_scope::has_dynamic_eval(function)
        || function.instructions.iter().any(|inst| inst.op == Op::CoreBuiltin)
        || function.locals.iter().any(|local| matches!(local.kind,
            LocalKind::RefCell | LocalKind::ReturnRefCell | LocalKind::EvalContext
            | LocalKind::EvalScope | LocalKind::EvalGlobalScope | LocalKind::GeneratorState))
    { return false; }
    let mut changed = seed::run(function, data, pass);
    for _ in 0..4 {
        if function.blocks.iter().filter(|b| b.name.ends_with(".counter.slow")).count() >= 4 { break; }
        let dominance = crate::ir_passes::compute_dominance(function);
        let loops = crate::ir_passes::loops::compute_loops(function, &dominance);
        let plan = loops.loops().iter().filter(|lp| {
            !function.blocks[lp.header.as_raw() as usize].name.contains(".counter.")
                && !loops.loops().iter().any(|inner| inner.header != lp.header && lp.contains(inner.header))
        }).find_map(|lp| analyze(function, lp, data, pass));
        let Some(plan) = plan else { break; };
        let mut trial = function.clone();
        apply(&mut trial, &plan);
        if crate::ir::validate_function(&trial).is_err() { break; }
        *function = trial;
        changed = true;
    }
    changed
}

/// The immutable loop inventory and an already scalarized copy of its instruction graph.
struct Plan {
    blocks: Vec<BlockId>,
    header: BlockId,
    preheader: BlockId,
    slot: LocalSlotId,
    bound: ValueId,
    predicate: CmpPredicate,
    scalarized: Function,
    bound_placeholder: ValueId,
}

/// Recognizes one step-one loop with a stable bound and no SSA values escaping its blocks.
fn analyze(function: &Function, lp: &NaturalLoop, data: &DataPool, pass: &BoundedMixedCounter) -> Option<Plan> {
    let preheader = lp.preheader?;
    let header = function.block(lp.header)?;
    if !header.params.is_empty() || lp.blocks.iter().map(|b| function.blocks[b.as_raw() as usize].instructions.len()).sum::<usize>() > 256 {
        return None;
    }
    let Terminator::CondBr { cond, then_target, else_target, then_args, else_args } = header.terminator.as_ref()? else { return None; };
    if !lp.contains(*then_target) || lp.contains(*else_target) || !then_args.is_empty() || !else_args.is_empty() { return None; }
    let comparison = defining_id(function, *cond)?;
    let compare = function.instruction(comparison)?;
    if compare.op != Op::PhpRelCmp || !header.instructions.contains(&comparison) { return None; }
    let Some(Immediate::CmpPredicate(mut predicate)) = compare.immediate else { return None; };
    let &[mut counter, mut bound] = compare.operands.as_slice() else { return None; };
    if counter_slot(function, counter).is_none() {
        std::mem::swap(&mut counter, &mut bound);
        predicate = match predicate {
            CmpPredicate::Slt => CmpPredicate::Sgt, CmpPredicate::Sle => CmpPredicate::Sge,
            CmpPredicate::Sgt => CmpPredicate::Slt, CmpPredicate::Sge => CmpPredicate::Sle, _ => return None,
        };
    }
    let slot = counter_slot(function, counter)?;
    if !header.instructions.contains(&defining_id(function, counter)?) { return None; }
    let local = function.locals.get(slot.as_raw() as usize)?;
    if local.kind != LocalKind::PhpLocal || local.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || (slot.as_raw() as usize) < function.params.len()
        || function.no_epilogue_cleanup_slots.contains(&slot) { return None; }
    let step = match predicate { CmpPredicate::Slt | CmpPredicate::Sle => 1, CmpPredicate::Sgt | CmpPredicate::Sge => -1, _ => return None };
    let dominance = crate::ir_passes::compute_dominance(function);
    let bound_block = match function.value(bound)?.def { ValueDef::Instruction { block, .. } | ValueDef::BlockParam { block, .. } => block };
    let bound_load = defining_instruction(function, bound).filter(|inst| inst.op == Op::LoadLocal);
    if lp.contains(bound_block) || bound_load.is_some() {
        let load = bound_load?;
        let Some(Immediate::LocalSlot(bound_slot)) = load.immediate else { return None; };
        let storage = function.locals.get(bound_slot.as_raw() as usize)?;
        if storage.ir_type != load.result_type || storage.php_type.codegen_repr() != load.result_php_type.codegen_repr()
            || bound_slot == slot
            || crate::ir_passes::address_escaping_slots(function).contains(&bound_slot)
            || lp.blocks.iter().any(|b| function.blocks[b.as_raw() as usize].instructions.iter().any(|id| {
                let inst = &function.instructions[id.as_raw() as usize];
                inst.immediate == Some(Immediate::LocalSlot(bound_slot)) && inst.op != Op::LoadLocal
            })) { return None; }
    }
    if !lp.contains(bound_block) && !dominance.dominates(bound_block, preheader) { return None; }
    let metadata = function.value(bound)?;
    if !matches!((&metadata.ir_type, &metadata.php_type),
        (IrType::I64, PhpType::Int) | (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)) { return None; }
    let mut stores = 0;
    let mut seed = false;
    for block in &function.blocks {
        for &id in &block.instructions {
            let inst = function.instruction(id)?;
            if inst.immediate != Some(Immediate::LocalSlot(slot)) { continue; }
            if lp.contains(block.id) && inst.op == Op::StoreLocal {
                // A post-test update can overflow before its header comparison rejects it.
                if block.id == lp.header { return None; }
                stores += 1;
                let update = storage::owner_source(function, *inst.operands.first()?)?;
                if update.op != Op::StrIncDec || update.immediate != Some(Immediate::I64(step)) { return None; }
            } else if !lp.contains(block.id) && inst.op == Op::StoreLocal {
                // Repeated or conditionally initialized epochs require their own binding first.
                if block.id != preheader || seed { return None; }
                seed = true;
            } else if !lp.contains(block.id) && inst.op == Op::LoadLocal {
                // Exit observations are allowed; the fast exit publishes the exact boxed integer.
            } else if !matches!(inst.op, Op::LoadLocal | Op::ReleaseLocalSlot) { return None; }
        }
    }
    if stores != 1 || !seed || clone::values_escape(function, lp) { return None; }
    let mut scalarized = function.clone();
    // Outside accesses keep their original representation in the final function. Erase them
    // only in this private inventory so they cannot constrain the fast loop's local proof.
    for block in &function.blocks {
        if lp.contains(block.id) { continue; }
        for &id in &block.instructions {
            if scalarized.instructions[id.as_raw() as usize].immediate == Some(Immediate::LocalSlot(slot)) {
                neutralize_to_nop(&mut scalarized.instructions[id.as_raw() as usize]);
            }
        }
    }
    let mut builder = Builder::new(&mut scalarized);
    let placeholder_block = builder.create_named_block("counter.proof", vec![]);
    builder.position_at_end(placeholder_block);
    let bound_placeholder = builder.emit_const_i64(0);
    builder.emit_store_local(slot, bound_placeholder);
    builder.terminate(Terminator::Unreachable);
    let compare = &mut scalarized.instructions[comparison.as_raw() as usize];
    compare.operands = vec![counter, bound_placeholder];
    compare.immediate = Some(Immediate::CmpPredicate(predicate));
    let proof = committed_proof(&scalarized, slot, pass, data)?;
    if proof.updates.len() != 1 { return None; }
    proof.scalarize(&mut scalarized, slot);
    // The guard and header predicate prove that the only step-one writer cannot overflow.
    for update in proof.updates {
        let inst = &mut scalarized.instructions[update.as_raw() as usize];
        inst.op = Op::IAdd; inst.effects = Op::IAdd.default_effects();
    }
    Some(Plan { blocks: lp.blocks.clone(), header: lp.header, preheader, slot, bound,
        predicate, scalarized, bound_placeholder })
}

/// Preserves consumer representations and PHP semantics when committing a scalar counter.
fn committed_proof(function: &Function, slot: LocalSlotId, pass: &BoundedMixedCounter, data: &DataPool) -> Option<CounterProof> {
    let proof = CounterProof::for_slots(function, &HashSet::from([slot]), pass, data, Some(&HashMap::new()))?;
    // Storage trials can use the broader value-only inventory because ordinary AST lowering
    // rebuilds their conversions. A committed copy keeps the old opcode, so fixed-box consumers
    // and coercing stores must retain the original generic loop instead.
    if function.instructions.iter().any(|inst| {
        inst.operands.iter().any(|value| proof.scalar_values.contains(value))
            && !committed_consumer(function, inst, &proof.scalar_values)
    }) { return None; }
    Some(proof)
}

/// Checks representation-sensitive casts and comparison partners as well as the consumer opcode.
fn committed_consumer(function: &Function, inst: &Instruction, scalar_values: &HashSet<ValueId>) -> bool {
    match inst.op {
        Op::Cast => matches!(inst.immediate,
            Some(Immediate::CastTarget(IrType::I64 | IrType::F64 | IrType::Str)
                | Immediate::StringOffsetCast)),
        Op::Spaceship | Op::StrictEq | Op::StrictNotEq | Op::LooseEq | Op::LooseNotEq =>
            super::consumers::comparison_preserves_integer(function, inst, scalar_values),
        op => matches!(op,
            Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone | Op::Release
            | Op::StoreLocal | Op::StrIncDec | Op::PhpRelCmp | Op::MixedNumericBinop
            | Op::Call | Op::ArrayPush | Op::MixedBox | Op::EchoValue | Op::IsTruthy),
    }
}

/// Returns only direct reads of a boxed local; arbitrary aliases are not counter identities.
fn counter_slot(function: &Function, value: ValueId) -> Option<LocalSlotId> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) { (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) => Some(*slot), _ => None }
}

/// Emits a typed instruction with the opcode's conservative effect contract.
fn emit(builder: &mut Builder<'_>, op: Op, operands: Vec<ValueId>, immediate: Option<Immediate>, ty: IrType, php: PhpType) -> ValueId {
    builder.emit(op, operands, immediate, ty, php.clone(), Ownership::for_php_type(&php)).unwrap()
}

/// Checks a dynamic value's tag before allowing its payload onto the integer path.
fn integer_guard(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    if builder.function().value(value).is_some_and(|v| v.ir_type == IrType::I64 && v.php_type == PhpType::Int) {
        return builder.emit_const_bool(true);
    }
    emit(builder, Op::TypePredicate, vec![value], Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Int)), IrType::I64, PhpType::Bool)
}

/// Inserts seed/tag/bound guards, copies the fast loop and synchronizes every normal exit.
fn apply(function: &mut Function, plan: &Plan) {
    let fast_slot = function.add_local(None, IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    function.blocks[plan.preheader.as_raw() as usize].terminator = None;
    let mut builder = Builder::new(function);
    builder.position_at_end(plan.preheader);
    let slow = plan.header;
    let seed = builder.emit(Op::LoadLocal, vec![], Some(Immediate::LocalSlot(plan.slot)),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Borrowed).unwrap();
    let seed_ok = integer_guard(&mut builder, seed);
    let bound = match builder.function().value(plan.bound).unwrap().def {
        ValueDef::Instruction { block, .. } if plan.blocks.contains(&block) => {
            let load = defining_instruction(builder.function(), plan.bound).unwrap();
            let immediate = load.immediate.clone();
            let ty = load.result_type; let php = load.result_php_type.clone();
            let ownership = if ty == IrType::I64 { Ownership::NonHeap } else { Ownership::Borrowed };
            builder.emit(Op::LoadLocal, vec![], immediate, ty, php, ownership).unwrap()
        }
        _ => plan.bound,
    };
    let bound_ok = integer_guard(&mut builder, bound);
    let bound_guard = builder.create_named_block("counter.bound.guard", vec![]);
    let integer_entry = builder.create_named_block("counter.integer.entry", vec![]);
    builder.terminate(Terminator::CondBr { cond: seed_ok, then_target: bound_guard, then_args: vec![], else_target: slow, else_args: vec![] });
    builder.position_at_end(bound_guard);
    builder.terminate(Terminator::CondBr { cond: bound_ok, then_target: integer_entry, then_args: vec![], else_target: slow, else_args: vec![] });
    builder.position_at_end(integer_entry);
    let seed_int = integer_payload(&mut builder, seed);
    let bound_int = integer_payload(&mut builder, bound);
    builder.emit_store_local(fast_slot, seed_int);
    let fast_entry = builder.create_named_block("counter.integer.ready", vec![]);
    if matches!(plan.predicate, CmpPredicate::Sle | CmpPredicate::Sge) {
        let edge = builder.emit_const_i64(if plan.predicate == CmpPredicate::Sle { i64::MAX } else { i64::MIN });
        let safe = emit(&mut builder, Op::ICmp, vec![bound_int, edge], Some(Immediate::CmpPredicate(CmpPredicate::Ne)), IrType::I64, PhpType::Bool);
        builder.terminate(Terminator::CondBr { cond: safe, then_target: fast_entry, then_args: vec![], else_target: slow, else_args: vec![] });
    } else { builder.terminate(Terminator::Br { target: fast_entry, args: vec![] }); }
    drop(builder);
    let header = clone::copy_loop(function, plan, fast_slot, bound_int);
    function.blocks[fast_entry.as_raw() as usize].terminator = Some(Terminator::Br { target: header, args: vec![] });
    function.blocks[plan.header.as_raw() as usize].name.push_str(".counter.slow");
}

/// Extracts a tag-guarded integer payload without introducing PHP cast warnings or scratch effects.
fn integer_payload(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    if builder.function().value(value).is_some_and(|v| v.ir_type == IrType::I64 && v.php_type == PhpType::Int) {
        return value;
    }
    emit(builder, Op::MixedUnbox, vec![value], None, IrType::I64, PhpType::Int)
}
