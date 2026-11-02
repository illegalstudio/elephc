//! Purpose:
//! Proves that an EIR value graph can contain only PHP integers or floats.
//!
//! Called from:
//! - `codegen::stack_numeric` before frame selection and scalar retirement.
//! - `ir_passes::concat_reset` before treating numeric updates as scratch-inert.
//! - `ir_passes::buffer_access::numeric` before treating numeric boxes as buffer-preserving.
//!
//! Key details:
//! - Every local writer participates in the proof, including seeds and loop backedges.
//! - Calls, parameters with unknown tags, references and nonnumeric values fail closed.
//! - Immutable slot access facts and definitive rejections are shared across queries.

use std::collections::{HashMap, HashSet};
use crate::ir::{BlockId, Function, Immediate, IrHeapKind, IrType, LocalKind, LocalSlotId, Op, ValueId};
use crate::types::PhpType;
use crate::ir::MixedNumericOp;

/// Accepts only the three arithmetic operations with exact integer overflow-to-float semantics.
pub(crate) fn mixed(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::MixedNumericBinop && matches!(inst.immediate,
        Some(Immediate::MixedNumericOp(MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul)))
}

/// Recognizes only the two unary steps whose numeric semantics are checked addition.
pub(crate) fn increment(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::StrIncDec && matches!(inst.immediate, Some(Immediate::I64(-1 | 1)))
}

/// Recognizes ordered numeric predicates whose unordered floating-point result is false.
pub(crate) fn comparison(inst: &crate::ir::Instruction) -> bool {
    use crate::ir::CmpPredicate::{Slt, Sle, Sgt, Sge};
    inst.op == Op::PhpRelCmp && matches!(inst.immediate, Some(Immediate::CmpPredicate(Slt | Sle | Sgt | Sge)))
}

/// Checks a closed numeric graph, constructing dominance only for otherwise numeric private slots.
/// Local cycles are accepted inductively only after every writer and dominating seed passes.
pub(crate) fn inputs(function: &Function, values: &[ValueId]) -> bool {
    NumericInputs::new(function).query(values, false)
}

/// Retains completed numeric facts for one immutably borrowed function.
/// Provisional cycle facts stay private until the entire conjunctive query succeeds.
pub(crate) struct NumericInputs<'a> {
    function: &'a Function,
    enabled: bool,
    proven: HashSet<ValueId>,
    rejected: HashSet<ValueId>,
    working: HashMap<ValueId, bool>,
    locals: NumericLocals,
}

/// Cached local facts; positive cycle assumptions are committed only after a whole query passes.
#[derive(Default)]
struct NumericLocals {
    proven: HashSet<LocalSlotId>,
    rejected: HashSet<LocalSlotId>,
    working: HashSet<LocalSlotId>,
    accesses: Option<Vec<LocalAccess>>,
    escaping: Option<HashSet<LocalSlotId>>,
    dominance: Option<crate::ir_passes::DominanceInfo>,
}

/// One slot's immutable writers, forbidden operations and live CFG access positions.
#[derive(Default)]
struct LocalAccess {
    forbidden: bool,
    writers: Vec<ValueId>,
    stores: Vec<(BlockId, usize)>,
    loads: Vec<(BlockId, usize)>,
}

impl<'a> NumericInputs<'a> {
    /// Binds all cached facts to an immutable function and its handler policy.
    pub(crate) fn new(function: &'a Function) -> Self {
        let enabled = !function.instructions.iter().any(|inst| matches!(inst.op,
            Op::TryPushHandler | Op::CatchBind | Op::FinallyEnter));
        Self { function, enabled, proven: HashSet::new(), rejected: HashSet::new(),
            working: HashMap::new(), locals: NumericLocals::default() }
    }

    /// Publishes only complete successes and definitive failures, never provisional facts.
    pub(crate) fn prove(&mut self, values: &[ValueId]) -> bool {
        self.query(values, true)
    }

    /// Avoids retaining a second value set when the caller needs only one query.
    fn query(&mut self, values: &[ValueId], retain: bool) -> bool {
        self.working.clear();
        self.locals.working.clear();
        if !self.enabled { return false; }
        let mut slots = HashSet::new();
        let safe = values.iter().all(|&value| numeric(
            self.function, value, &mut slots, &mut self.locals, &self.proven, &self.rejected, &mut self.working,
        ));
        if safe && retain {
            self.proven.extend(self.working.iter().filter_map(|(&value, &safe)| safe.then_some(value)));
            self.locals.proven.extend(self.locals.working.drain());
        }
        self.rejected.extend(self.working.iter().filter_map(|(&value, &safe)| (!safe).then_some(value)));
        self.working.clear();
        self.locals.working.clear();
        safe
    }

    /// Freezes complete facts for codegen after the function's frame proof is finished.
    pub(crate) fn into_proven_values(self) -> HashSet<ValueId> {
        self.proven
    }
}

/// Shares value decisions within one conjunctive proof, avoiding exponential shared-DAG walks.
///
/// Local cycles may provisionally succeed while their writers are visited. Every writer
/// must succeed for the whole proof to pass; provisional successes never escape a failed query.
fn numeric(function: &Function, value: ValueId, slots: &mut HashSet<LocalSlotId>, locals: &mut NumericLocals, proven: &HashSet<ValueId>, rejected: &HashSet<ValueId>, known: &mut HashMap<ValueId, bool>) -> bool {
    if proven.contains(&value) { return true; }
    if rejected.contains(&value) { return false; }
    if let Some(&safe) = known.get(&value) { return safe; }
    let safe = numeric_uncached(function, value, slots, locals, proven, rejected, known);
    known.insert(value, safe);
    safe
}

/// Requires scalar metadata or a boxed value whose complete origin is numeric.
fn numeric_uncached(function: &Function, value: ValueId, slots: &mut HashSet<LocalSlotId>, locals: &mut NumericLocals, proven: &HashSet<ValueId>, rejected: &HashSet<ValueId>, known: &mut HashMap<ValueId, bool>) -> bool {
    let Some(metadata) = function.value(value) else { return false; };
    if matches!((&metadata.ir_type, &metadata.php_type),
        (IrType::I64, PhpType::Int) | (IrType::F64, PhpType::Float))
    {
        return true;
    }
    if metadata.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || metadata.php_type.codegen_repr() != PhpType::Mixed
    {
        return false;
    }
    let Some(inst) = super::rewrite::defining_instruction(function, value) else { return false; };
    match inst.op {
        Op::Acquire | Op::MixedClone | Op::Move | Op::Borrow | Op::EnsureOwned | Op::MixedBox => {
            inst.operands.first().is_some_and(|&value| numeric(function, value, slots, locals, proven, rejected, known))
        }
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul => {
            inst.operands.len() == 2 && inst.operands.iter().all(|&value| numeric(function, value, slots, locals, proven, rejected, known))
        }
        Op::MixedNumericBinop if mixed(inst) => {
            inst.operands.len() == 2 && inst.operands.iter().all(|&value| numeric(function, value, slots, locals, proven, rejected, known))
        }
        Op::StrIncDec if increment(inst) => {
            inst.operands.len() == 1 && numeric(function, inst.operands[0], slots, locals, proven, rejected, known)
        }
        Op::LoadLocal => {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return false; };
            let Some(local) = function.locals.get(slot.as_raw() as usize) else { return false; };
            if local.kind != LocalKind::PhpLocal || (slot.as_raw() as usize) < function.params.len() {
                return false;
            }
            if locals.rejected.contains(&slot) { return false; }
            // Active and working slots are optimistic cycle facts, private to this query.
            if locals.proven.contains(&slot) || locals.working.contains(&slot) || slots.contains(&slot) {
                return true;
            }
            #[cfg(test)]
            SLOT_VALIDATIONS.with(|checks| checks.set(checks.get() + 1));
            if locals.escaping.get_or_insert_with(|| crate::ir_passes::address_escaping_slots(function)).contains(&slot) {
                locals.rejected.insert(slot);
                return false;
            }
            let access = &locals.accesses.get_or_insert_with(|| local_accesses(function))[slot.as_raw() as usize];
            if access.forbidden {
                locals.rejected.insert(slot);
                return false;
            }
            let writers = access.writers.clone();
            slots.insert(slot);
            let safe = !writers.is_empty() && writers.iter().all(|&value| {
                numeric(function, value, slots, locals, proven, rejected, known)
            }) && initialized(&locals.accesses.as_ref().unwrap()[slot.as_raw() as usize], locals.dominance.get_or_insert_with(|| {
                crate::ir_passes::compute_dominance(function)
            }));
            slots.remove(&slot);
            if safe { locals.working.insert(slot); }
            else {
                // Failure with optimistic cycle assumptions is definitive for this immutable function.
                locals.rejected.insert(slot);
            }
            safe
        }
        _ => false,
    }
}

/// Requires a dominating store before every read, including reads feeding another slot's writer.
/// The enclosing origin proof checks every stored value's numeric domain separately, so a
/// computed numeric temporary can initialize its slot without requiring a literal seed.
fn initialized(access: &LocalAccess, dominance: &crate::ir_passes::DominanceInfo) -> bool {
    access.loads.iter().all(|&(block, position)| access.stores.iter().any(|&(seed_block, seed_position)| {
        if seed_block == block { seed_position < position }
        else { dominance.dominates(seed_block, block) }
    }))
}

/// Indexes every slot once, retaining the old all-instructions writer and live-CFG seed policy.
fn local_accesses(function: &Function) -> Vec<LocalAccess> {
    let mut accesses: Vec<_> = (0..function.locals.len()).map(|_| LocalAccess::default()).collect();
    for inst in &function.instructions {
        match inst.immediate {
            Some(Immediate::LocalSlot(slot)) => {
                let Some(access) = accesses.get_mut(slot.as_raw() as usize) else { continue; };
                match inst.op {
                    Op::StoreLocal => match inst.operands.first() {
                        Some(&value) => access.writers.push(value),
                        None => access.forbidden = true,
                    },
                    Op::LoadLocal | Op::ReleaseLocalSlot => {},
                    _ => access.forbidden = true,
                }
            }
            Some(Immediate::LocalSlotPair { first, second }) => {
                for slot in [first, second] {
                    if let Some(access) = accesses.get_mut(slot.as_raw() as usize) { access.forbidden = true; }
                }
            }
            _ => {},
        }
    }
    for block in &function.blocks {
        for (position, &id) in block.instructions.iter().enumerate() {
            let inst = &function.instructions[id.as_raw() as usize];
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { continue; };
            let Some(access) = accesses.get_mut(slot.as_raw() as usize) else { continue; };
            if inst.op == Op::LoadLocal { access.loads.push((block.id, position)); }
            if inst.op == Op::StoreLocal { access.stores.push((block.id, position)); }
        }
    }
    accesses
}

#[cfg(test)]
std::thread_local! {
    /// Counts expensive slot eligibility checks within this test thread's proof.
    static SLOT_VALIDATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, Ownership, Terminator};

    /// Builds a shared local cycle and returns its final load and scalar witnesses.
    fn shared_cycle() -> (Function, ValueId, ValueId, ValueId) {
        let mut function = Function::new("numeric_domain_work".into(), IrType::I64, PhpType::Int);
        let slot = function.add_local(
            Some("sum".into()), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, LocalKind::PhpLocal,
        );
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let zero = builder.emit_const_i64(0);
        let one = builder.emit_const_i64(1);
        let nonnumeric = builder.emit_const_bool(false);
        builder.emit_store_local(slot, zero);
        for _ in 0..128 {
            let previous = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            let next = builder.emit(
                Op::MixedNumericBinop, vec![previous, one],
                Some(Immediate::MixedNumericOp(MixedNumericOp::Add)),
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
            ).unwrap();
            builder.emit_store_local(slot, next);
        }
        let loaded = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        builder.terminate(Terminator::Return { value: Some(zero) });
        (function, loaded, nonnumeric, one)
    }

    /// A tag-only predicate observes the value without exposing its private numeric slot.
    #[test]
    fn numeric_domain_type_predicate_preserves_private_slot() {
        let (mut function, loaded, _, _) = shared_cycle();
        let entry = function.entry;
        let term = function.blocks[entry.as_raw() as usize].terminator.take().unwrap();
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(entry);
        builder.emit(Op::TypePredicate, vec![loaded],
            Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Int)),
            IrType::I64, PhpType::Bool, Ownership::NonHeap);
        builder.terminate(term);
        assert!(crate::ir::validate_function(&function).is_ok());
        assert!(inputs(&function, &[loaded]));
    }

    /// Shared local cycles validate accesses once while retaining all-writer checks.
    #[test]
    fn numeric_domain_reuses_prechecked_slot_eligibility() {
        let (mut function, loaded, nonnumeric, _) = shared_cycle();
        SLOT_VALIDATIONS.with(|checks| checks.set(0));
        assert!(inputs(&function, &[loaded]));
        assert_eq!(SLOT_VALIDATIONS.with(|checks| checks.get()), 1,
            "revisited loads must reuse the active slot's validated accesses");

        let mut tainted = function.clone();
        let writer = tainted.instructions.iter_mut().rev().find(|inst| inst.op == Op::StoreLocal).unwrap();
        writer.operands[0] = nonnumeric;
        assert!(!inputs(&tainted, &[loaded]), "a later nonnumeric writer must still reject the whole cycle");

        let seed = function.instructions.iter_mut().find(|inst| inst.op == Op::StoreLocal).unwrap();
        seed.op = Op::Nop;
        seed.immediate = None;
        seed.operands.clear();
        seed.effects = Op::Nop.default_effects();
        assert!(!inputs(&function, &[loaded]), "an active cycle must not initialize its own entry value");
    }

    /// Complete facts survive unrelated failures and eliminate repeated function-local queries.
    #[test]
    fn numeric_domain_reuses_only_completed_function_facts() {
        let (function, loaded, nonnumeric, _) = shared_cycle();
        let mut proof = NumericInputs::new(&function);
        SLOT_VALIDATIONS.with(|checks| checks.set(0));
        assert!(proof.prove(&[loaded]));
        let completed = proof.proven.clone();
        for inst in function.instructions.iter().filter(|inst| inst.op == Op::LoadLocal) {
            assert!(proof.prove(&[inst.result.unwrap()]));
        }
        assert!(!proof.prove(&[nonnumeric]));
        assert_eq!(proof.proven, completed);
        assert!(proof.working.is_empty());
        assert!(proof.prove(&[loaded]));
        assert_eq!(SLOT_VALIDATIONS.with(|checks| checks.get()), 1);
    }

    /// Failed cycles and conjunctions never publish provisional successes into later queries.
    #[test]
    fn numeric_domain_failed_queries_do_not_publish_cycle_facts() {
        let (mut function, loaded, nonnumeric, one) = shared_cycle();
        let first_load = function.instructions.iter().find(|inst| inst.op == Op::LoadLocal).unwrap().result.unwrap();
        let writer = function.instructions.iter_mut().rev().find(|inst| inst.op == Op::StoreLocal).unwrap();
        writer.operands[0] = nonnumeric;
        let mut proof = NumericInputs::new(&function);
        assert!(proof.prove(&[one]));
        assert!(!proof.prove(&[loaded]));
        assert_eq!(proof.proven, HashSet::from([one]));
        assert!(proof.working.is_empty());
        assert!(!proof.prove(&[first_load]));
        assert_eq!(proof.proven, HashSet::from([one]));

        let (valid, loaded, nonnumeric, _) = shared_cycle();
        let mut proof = NumericInputs::new(&valid);
        assert!(!proof.prove(&[loaded, nonnumeric]));
        assert!(proof.proven.is_empty());
        assert!(proof.working.is_empty());
        assert!(proof.locals.proven.is_empty());
        assert!(proof.locals.working.is_empty());
        assert!(proof.prove(&[loaded]));
    }

    /// Distinct loads of one accepted or rejected slot reuse its complete eligibility decision.
    #[test]
    fn numeric_domain_reuses_slot_verdicts_for_independent_loads() {
        for valid in [true, false] {
            let (mut function, _, nonnumeric, _) = shared_cycle();
            if !valid {
                let seed = function.instructions.iter_mut().find(|inst| inst.op == Op::StoreLocal).unwrap();
                seed.operands[0] = nonnumeric;
            }
            let end = function.blocks[0].terminator.take().unwrap();
            let mut b = Builder::new(&mut function);
            b.position_at_end(crate::ir::BlockId::from_raw(0));
            let loads: Vec<_> = (0..128).map(|_| b.emit_load_local(
                LocalSlotId::from_raw(0), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed,
            )).collect();
            b.terminate(end);
            let mut proof = NumericInputs::new(&function);
            SLOT_VALIDATIONS.with(|checks| checks.set(0));
            for load in loads {
                assert_eq!(proof.prove(&[load]), valid);
            }
            assert_eq!(SLOT_VALIDATIONS.with(|checks| checks.get()), 1,
                "accepted and rejected slots must not rescan the function for each read");
        }
    }
}
