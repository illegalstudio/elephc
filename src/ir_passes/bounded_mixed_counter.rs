//! Purpose:
//! Promotes a boxed incrementing local only after proving every update fits in an integer.
//!
//! Called from:
//! - The EIR fixed-point driver before ordinary loop and integer-range simplification.
//!
//! Key details:
//! - A private trial function is scalarized, promoted to SSA and checked by IntegerRange.
//! - Unproven overflow, type changes, references, implicit scope access and pointer escapes fail closed.
//! - Both `$i++`/`$i--` and constant compound steps (`$i += c`, `$i -= c`) are counter updates.
//! - Private slots in the executable main frame are eligible; a runtime `include` that exposes
//!   the frame, or dynamic `eval`, keeps them boxed.
//! - Existing direct calls keep their target and ABI; this pass never authorizes a closure call.
//! - A seed computed by checked or mixed `+ - *` (`$j = $i + 1`) must be proven unchecked like
//!   an update, and a comparison against a checked `+`/`-` bound (`$i < $end - 1`) stays a PHP
//!   comparison whose one-sided overflow IntegerRange reasons about.

use std::collections::{HashMap, HashSet};

use super::driver::IrPass;
use super::rewrite::{defining_instruction, neutralize_to_nop, replace_all_uses, resolve_chains};
use crate::ir::{
    DataPool, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalKind, LocalSlotId,
    MixedNumericOp, Module, Op, Ownership, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

mod storage;
mod consumers;
mod versioning;
pub(super) use versioning::LoopCounterVersioning;
pub(crate) use storage::prove_integer_slots;

/// Narrows only counters for which the existing range analysis proves all updates safe.
#[derive(Default)]
pub struct BoundedMixedCounter {
    mixed_parameters: HashMap<String, Vec<bool>>,
}

impl BoundedMixedCounter {
    /// Captures physical Mixed by-value argument positions before function passes run.
    pub(super) fn new(module: &Module) -> Self {
        let mut mixed_parameters = HashMap::new();
        // Match FunctionContext::function_by_name: PHP canonical names, free functions
        // before closures, and the first match even when that target is ineligible.
        // Other function tables and variant-dispatch fallbacks stay outside this proof.
        for function in module.functions.iter().chain(&module.closures) {
            let key = crate::names::php_symbol_key(function.name.trim_start_matches('\\'));
            mixed_parameters.entry(key).or_insert_with(|| {
                let eligible = !function.flags.by_ref_return
                    && !function.flags.is_generator
                    && !function.flags.is_fiber_wrapper;
                function
                    .params
                    .iter()
                    .map(|param| {
                        eligible
                            && !param.by_ref
                            && !param.variadic
                            && param.php_type == PhpType::Mixed
                            && param.ir_type == IrType::Heap(IrHeapKind::Mixed)
                    })
                    .collect()
            });
        }
        Self { mixed_parameters }
    }

    /// Requires the exact called parameter to retain its ordinary boxed Mixed ABI.
    fn safe_call_use(&self, inst: &Instruction, value: ValueId, data: &DataPool) -> bool {
        let Some(Immediate::Data(name)) = inst.immediate else {
            return false;
        };
        let Some(parameters) = data
            .function_names
            .get(name.as_raw() as usize)
            .and_then(|name| {
                let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
                self.mixed_parameters.get(&key)
            })
        else {
            return false;
        };
        inst.operands.len() == parameters.len()
            && inst
                .operands
                .iter()
                .enumerate()
                .filter(|(_, operand)| **operand == value)
                .all(|(index, _)| parameters[index])
    }
}

impl IrPass for BoundedMixedCounter {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "bounded_mixed_counter"
    }

    /// Avoids graph construction unless a boxed local has a recognized counter update.
    fn is_applicable(&self, function: &Function) -> bool {
        let boxed: HashSet<LocalSlotId> = function
            .locals
            .iter()
            .filter(|local| {
                local.kind == LocalKind::PhpLocal
                    && local.ir_type == IrType::Heap(IrHeapKind::Mixed)
                    && local.php_type == PhpType::Mixed
            })
            .map(|local| local.id)
            .collect();
        if boxed.is_empty() {
            return false;
        }
        function.instructions.iter().any(|inst| {
            if inst.op != Op::StoreLocal {
                return false;
            }
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                return false;
            };
            boxed.contains(&slot)
                && inst
                    .operands
                    .first()
                    .is_some_and(|&stored| stored_counter_update(function, slot, stored))
        })
    }

    /// Commits a trial representation only when every new checked update becomes unchecked.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if (function.flags.is_main && !main_frame_is_private(function, data))
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || function.instructions.len() > 4096
            || super::cfg::has_exception_handlers(function)
            || super::local_scope::has_dynamic_eval(function)
            || function.locals.iter().any(|local| {
                matches!(
                    local.kind,
                    LocalKind::RefCell
                        | LocalKind::ReturnRefCell
                        | LocalKind::EvalContext
                        | LocalKind::EvalScope
                        | LocalKind::EvalGlobalScope
                        | LocalKind::GeneratorState
                )
            })
        {
            return false;
        }
        let candidates: Vec<_> = function
            .locals
            .iter()
            .filter(|local| {
                local.kind == LocalKind::PhpLocal
                    && local.ir_type == IrType::Heap(IrHeapKind::Mixed)
                    && local.php_type == PhpType::Mixed
                    && local.id.as_raw() as usize >= function.params.len()
                    && !function.no_epilogue_cleanup_slots.contains(&local.id)
            })
            .map(|local| local.id)
            .take(16)
            .collect();
        let mut changed = false;
        for slot in candidates {
            let Some(proof) = CounterProof::new(function, slot, self, data) else {
                continue;
            };
            let mut trial = function.clone();
            proof.scalarize(&mut trial, slot);
            if crate::ir::validate_function(&trial).is_err() {
                continue;
            }
            super::mem2reg::Mem2Reg.run(&mut trial, data);
            // Promotion checks definite assignment and implicit aliases. A remaining
            // addressable load/store cannot inherit an integer domain just from its label.
            if trial.instructions.iter().any(|inst| {
                matches!(inst.op, Op::LoadLocal | Op::StoreLocal)
                    && inst.immediate == Some(Immediate::LocalSlot(slot))
            }) {
                continue;
            }
            super::integer_range::IntegerRange.run(&mut trial, data);
            if proof.proven_unchecked(&trial) && crate::ir::validate_function(&trial).is_ok() {
                *function = trial;
                changed = true;
            }
        }
        changed
    }
}

/// Complete storage and consumer inventory for one candidate local.
struct CounterProof {
    scalar_values: HashSet<ValueId>,
    updates: HashSet<InstId>,
    comparisons: HashSet<InstId>,
    /// Checked or mixed `+ - *` producers whose result seeds a counter (`$j = $i + 1`). The
    /// trial must prove each one unchecked, exactly like an update.
    seeds: HashSet<InstId>,
    /// The candidate slots one of `seeds` writes.
    seeded_slots: HashSet<LocalSlotId>,
    releases: HashSet<InstId>,
    copy_readonly_calls: bool,
}

impl CounterProof {
    /// Requires integer seeds and a closed graph of loads, increments and transparent owners.
    fn new(
        function: &Function,
        slot: LocalSlotId,
        pass: &BoundedMixedCounter,
        data: &DataPool,
    ) -> Option<Self> {
        Self::with_readonly_calls(function, slot, pass, data, None)
    }

    /// Allows extra by-value observations only in a discarded storage-analysis trial.
    fn with_readonly_calls(
        function: &Function,
        slot: LocalSlotId,
        pass: &BoundedMixedCounter,
        data: &DataPool,
        readonly_calls: Option<&HashMap<InstId, HashSet<usize>>>,
    ) -> Option<Self> {
        Self::for_slots(function, &HashSet::from([slot]), pass, data, readonly_calls)
    }

    /// Checks mutually observed counters together; every seed and update must remain integer.
    fn for_slots(
        function: &Function,
        slots: &HashSet<LocalSlotId>,
        pass: &BoundedMixedCounter,
        data: &DataPool,
        readonly_calls: Option<&HashMap<InstId, HashSet<usize>>>,
    ) -> Option<Self> {
        let mut proof = Self {
            scalar_values: HashSet::new(),
            updates: HashSet::new(),
            comparisons: HashSet::new(),
            seeds: HashSet::new(),
            seeded_slots: HashSet::new(),
            releases: HashSet::new(),
            copy_readonly_calls: readonly_calls.is_some(),
        };
        let mut uses: HashMap<ValueId, Vec<InstId>> = HashMap::new();
        let mut stores = Vec::new();
        let mut pending = Vec::new();
        let compound_updates = compound_counter_updates(function, slots);
        for block in &function.blocks {
            for &id in &block.instructions {
                let inst = function.instruction(id)?;
                for &operand in &inst.operands {
                    uses.entry(operand).or_default().push(id);
                }
                match inst.immediate {
                    Some(Immediate::LocalSlot(candidate)) if slots.contains(&candidate) => match inst.op {
                        Op::LoadLocal => {
                            let result = inst.result?;
                            let value = function.value(result)?;
                            if !matches!(
                                (&value.ir_type, &value.php_type),
                                (IrType::I64, PhpType::Int)
                                    | (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)
                            ) {
                                return None;
                            }
                            pending.push(result);
                        }
                        Op::StoreLocal => stores.push((candidate, *inst.operands.first()?)),
                        Op::ReleaseLocalSlot => {
                            proof.releases.insert(id);
                        }
                        _ => return None,
                    },
                    Some(Immediate::LocalSlotPair { first, second })
                        if slots.contains(&first) || slots.contains(&second) =>
                    {
                        return None
                    }
                    Some(Immediate::IterStart(ref iter))
                        if iter.local_slots().any(|slot| slots.contains(&slot)) =>
                    {
                        return None
                    }
                    _ => {}
                }
            }
        }
        while let Some(value) = pending.pop() {
            if !proof.scalar_values.insert(value) {
                continue;
            }
            if function
                .blocks
                .iter()
                .filter_map(|block| block.terminator.as_ref())
                .any(|term| term.used_values().contains(&value))
            {
                return None;
            }
            for &id in uses.get(&value).into_iter().flatten() {
                let inst = function.instruction(id)?;
                match inst.op {
                    Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone
                        if inst.immediate.is_none() =>
                    {
                        pending.push(inst.result?)
                    }
                    Op::Release => {
                        proof.releases.insert(id);
                    }
                    Op::StoreLocal if matches!(inst.immediate,
                        Some(Immediate::LocalSlot(slot)) if slots.contains(&slot)) => {}
                    Op::StrIncDec if matches!(inst.immediate, Some(Immediate::I64(1 | -1))) => {
                        proof.updates.insert(id);
                        pending.push(inst.result?);
                    }
                    _ if compound_updates.contains(&id) => {
                        proof.updates.insert(id);
                        pending.push(inst.result?);
                    }
                    Op::PhpRelCmp => {
                        proof.comparisons.insert(id);
                    }
                    // The lowering replay may already have narrowed the bound test while the
                    // slot was still integer; the range proof below keeps that test justified.
                    Op::ICmp => {}
                    // A conversion of the proven integer reads its bits without retaining the
                    // slot. `$data[] = $i % 97` casts the counter to int before `ismod`.
                    Op::Cast if observational_cast(inst) => {}
                    // Integer arithmetic reads the proven scalar bits by value; a result stored
                    // back into the slot still has to pass the seed check below.
                    Op::ISMod
                    | Op::IDiv
                    | Op::ISDiv
                    | Op::IAdd
                    | Op::ISub
                    | Op::IMul
                    | Op::IBitAnd
                    | Op::IBitOr
                    | Op::IBitXor
                    | Op::IShl
                    | Op::IShrA
                    | Op::INeg
                    | Op::IBitNot => {}
                    // A proven integer remains the same PHP array key after scalarization, and
                    // the typed index path is strictly cheaper than a boxed-key lookup.
                    Op::ArrayGet | Op::ArrayGetSilent
                        if inst.operands.get(1) == Some(&value) => {}
                    // Numeric helpers borrow their inputs. The unchanged opcode accepts
                    // a raw integer after the recurrence's complete range proof succeeds.
                    Op::MixedNumericBinop if matches!(inst.immediate,
                        Some(Immediate::MixedNumericOp(MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))) => {}
                    // Lowering already fixed the physical call signature and any by-ref storage.
                    // Re-materialization boxes a proven integer for an unchanged Mixed parameter.
                    Op::Call if pass.safe_call_use(inst, value, data) => {}
                    // These typed runtime operations accept a concrete integer operand and take
                    // the same fast path the explicit-cast form already produces. Keeping the
                    // counter scalar avoids boxing every loop iteration just to unbox it again.
                    Op::RuntimeCall if runtime_call_accepts_integer_counter(inst, value) => {}
                    _ if readonly_calls.is_some_and(|calls| {
                        calls.get(&id).is_some_and(|positions| {
                            inst.operands.iter().enumerate()
                                .filter(|(_, operand)| **operand == value)
                                .all(|(index, _)| positions.contains(&index))
                        })
                    }) => {}
                    _ if readonly_calls.is_some()
                        && (super::by_ref_alias::op_is_value_only_consumer(inst.op)
                            || function.is_packed_field_access(inst))
                        && !matches!(inst.op, Op::StoreLocal | Op::StoreGlobal
                            | Op::StoreStaticLocal | Op::InitStaticLocal | Op::StoreRefCell
                            | Op::ExternGlobalStore) => {}
                    Op::ArrayPush if readonly_calls.is_some()
                        && inst.operands.first().is_some_and(|operand| *operand != value) => {}
                    _ => return None,
                }
            }
        }
        if proof.updates.is_empty() || stores.is_empty() {
            return None;
        }
        for &(slot, value) in &stores {
            if !proof.scalar_values.contains(&value)
                && !integer(function, value)
                && !proof.accept_seed(function, slot, value, &uses, slots)
            {
                return None;
            }
        }
        let mut bound_comparisons = HashSet::new();
        for &id in &proof.comparisons {
            let operands = &function.instruction(id)?.operands;
            if operands
                .iter()
                .all(|&value| proof.scalar_values.contains(&value) || integer(function, value))
            {
                continue;
            }
            // `$i < $end - 1`: a counter against a checked `+`/`-` of raw integers. It stays a
            // PHP comparison, because the bound may be the float an overflow produced, and
            // IntegerRange decides whether the bound still limits the counter.
            if operands.len() == 2
                && operands.iter().any(|value| proof.scalar_values.contains(value))
                && operands
                    .iter()
                    .all(|&value| proof.scalar_values.contains(&value) || checked_bound(function, value))
            {
                bound_comparisons.insert(id);
                continue;
            }
            return None;
        }
        // Only the counter-against-counter or counter-against-integer tests become `icmp`.
        proof.comparisons.retain(|id| !bound_comparisons.contains(id));
        // An increment must consume a load or another closed numeric increment, not an
        // unrelated string value that merely shares a destination at one source location.
        // A compound update is a binary add/sub whose counter operand is closed and whose
        // step is a raw integer constant.
        if !proof.updates.iter().all(|id| {
            function.instruction(*id).is_some_and(|inst| {
                if inst.op == Op::StrIncDec {
                    inst.operands.len() == 1 && proof.scalar_values.contains(&inst.operands[0])
                } else {
                    inst.operands.len() == 2
                        && inst
                            .operands
                            .iter()
                            .any(|value| proof.scalar_values.contains(value))
                        && inst.operands.iter().all(|value| {
                            proof.scalar_values.contains(value) || integer(function, *value)
                        })
                }
            })
        }) {
            return None;
        }
        // Relowering can rebuild representation conversions, but comparison dispatch still
        // depends on the counter's type. Check after closing the graph so jointly proven
        // counters do not look like unrelated Mixed partners merely due to traversal order.
        if function.instructions.iter().any(|inst| {
            inst.operands.iter().any(|value| proof.scalar_values.contains(value))
                && !consumers::comparison_preserves_integer(function, inst, &proof.scalar_values)
        }) { return None; }
        Some(proof)
    }

    /// Accepts a store into `slot` of `wrapper*(P)`, where `P` is a checked or mixed `+`, `-`
    /// or `*` whose operands are raw integers or values of the counters being proven
    /// (`$j = $i + 1`).
    ///
    /// Each wrapper (acquire, move, borrow, clone) may only feed the next one, a release, or
    /// this store, and `P` may only feed the first wrapper or a release, so scalarizing `P`
    /// leaves no boxed observer behind. The wrappers become scalar copies, their releases
    /// disappear, and the trial must then prove `P` unchecked (`proven_unchecked`), which is
    /// what makes every value stored into the slot an integer.
    fn accept_seed(
        &mut self,
        function: &Function,
        slot: LocalSlotId,
        stored: ValueId,
        uses: &HashMap<ValueId, Vec<InstId>>,
        slots: &HashSet<LocalSlotId>,
    ) -> bool {
        let mut wrappers = Vec::new();
        let mut wrapper_insts = HashSet::new();
        let mut value = stored;
        let (producer_id, producer) = loop {
            let Some(id) = defining_id(function, value) else {
                return false;
            };
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            match inst.op {
                Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone
                    if inst.immediate.is_none() && wrappers.len() < 8 =>
                {
                    let Some(&source) = inst.operands.first() else {
                        return false;
                    };
                    wrappers.push(value);
                    wrapper_insts.insert(id);
                    value = source;
                }
                _ if seed_op(inst).is_some() => break (id, inst),
                _ => return false,
            }
        };
        let &[lhs, rhs] = producer.operands.as_slice() else {
            return false;
        };
        if ![lhs, rhs]
            .iter()
            .all(|&operand| self.scalar_values.contains(&operand) || integer(function, operand))
        {
            return false;
        }
        let Some(result) = producer.result else {
            return false;
        };
        let terminators: Vec<_> = function
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .collect();
        let mut releases = Vec::new();
        for owner in wrappers.iter().copied().chain([result]) {
            if terminators.iter().any(|term| term.used_values().contains(&owner)) {
                return false;
            }
            for &id in uses.get(&owner).into_iter().flatten() {
                let Some(user) = function.instruction(id) else {
                    return false;
                };
                match user.op {
                    Op::Release => releases.push(id),
                    Op::StoreLocal
                        if owner == stored
                            && matches!(user.immediate,
                                Some(Immediate::LocalSlot(target)) if slots.contains(&target)) => {}
                    _ if wrapper_insts.contains(&id) => {}
                    _ => return false,
                }
            }
        }
        self.seeds.insert(producer_id);
        self.seeded_slots.insert(slot);
        self.scalar_values.extend(wrappers);
        self.releases.extend(releases);
        true
    }

    /// Returns whether the trial proved every update and every seed free of overflow.
    fn proven_unchecked(&self, trial: &Function) -> bool {
        self.updates.iter().all(|id| {
            trial
                .instruction(*id)
                .is_some_and(|inst| matches!(inst.op, Op::IAdd | Op::ISub))
        }) && self.seeds.iter().all(|id| {
            trial
                .instruction(*id)
                .is_some_and(|inst| matches!(inst.op, Op::IAdd | Op::ISub | Op::IMul))
        })
    }

    /// Rewrites the private trial while keeping original instruction identifiers stable.
    fn scalarize(&self, function: &mut Function, slot: LocalSlotId) {
        self.scalarize_slots(function, &HashSet::from([slot]));
    }

    /// Rewrites a jointly proved trial once, preserving each original update identifier.
    fn scalarize_slots(&self, function: &mut Function, slots: &HashSet<LocalSlotId>) {
        for slot in slots {
            function.locals[slot.as_raw() as usize].ir_type = IrType::I64;
            function.locals[slot.as_raw() as usize].php_type = PhpType::Int;
        }
        for &value in &self.scalar_values {
            let metadata = &mut function.values[value.as_raw() as usize];
            metadata.ir_type = IrType::I64;
            metadata.php_type = PhpType::Int;
            metadata.ownership = Ownership::NonHeap;
            if let ValueDef::Instruction { inst, .. } = metadata.def {
                let inst = &mut function.instructions[inst.as_raw() as usize];
                inst.result_type = IrType::I64;
                inst.result_php_type = PhpType::Int;
                inst.result_ownership = Ownership::NonHeap;
                if matches!(inst.op, Op::Acquire | Op::Borrow | Op::MixedClone) {
                    inst.op = Op::Move;
                }
                inst.effects = inst.op.default_effects();
            }
        }
        // A seed producer becomes the checked integer form whose overflow the trial must rule
        // out; `proven_unchecked` rejects the trial unless IntegerRange made it unchecked.
        for &id in &self.seeds {
            let inst = &mut function.instructions[id.as_raw() as usize];
            let Some(op) = seed_op(inst) else {
                continue;
            };
            inst.op = op;
            inst.immediate = None;
            inst.result_type = IrType::I64;
            inst.result_php_type = PhpType::Int;
            inst.result_ownership = Ownership::NonHeap;
            inst.effects = op.default_effects();
            if let Some(result) = inst.result {
                let metadata = &mut function.values[result.as_raw() as usize];
                metadata.ir_type = IrType::I64;
                metadata.php_type = PhpType::Int;
                metadata.ownership = Ownership::NonHeap;
            }
        }
        for &id in &self.releases {
            neutralize_to_nop(&mut function.instructions[id.as_raw() as usize]);
        }
        for &id in &self.comparisons {
            let inst = &mut function.instructions[id.as_raw() as usize];
            inst.op = Op::ICmp;
            inst.effects = Op::ICmp.default_effects();
        }
        // Transparent owners now hold scalar bits. Redirect their uses through the
        // shared rewrite helpers, without running unrelated folds or mutating the data pool
        // of a rejected trial function.
        let mut copies = HashMap::new();
        let mut folded = Vec::new();
        for (index, inst) in function.instructions.iter().enumerate() {
            let id = InstId::from_raw(index as u32);
            if inst.op == Op::Move
                && inst
                    .result
                    .is_some_and(|value| self.scalar_values.contains(&value))
            {
                if let (Some(result), Some(&source)) = (inst.result, inst.operands.first()) {
                    copies.insert(result, source);
                    folded.push(id);
                }
            }
        }
        replace_all_uses(function, &resolve_chains(&copies));
        for id in folded {
            neutralize_to_nop(&mut function.instructions[id.as_raw() as usize]);
        }

        // The delta constants precede their update in the same block. Reindex all value
        // definitions after insertion, including pre-existing instructions later in the block.
        for block_index in 0..function.blocks.len() {
            let old = function.blocks[block_index].instructions.clone();
            let mut rewritten = Vec::with_capacity(old.len());
            for id in old {
                // A proven by-value Mixed argument receives copied scalar bits. Preserve
                // that explicit boundary until Mem2Reg has removed the source slot, rather
                // than relaxing the shared default-deny by-reference escape analysis.
                if function.instructions[id.as_raw() as usize].op == Op::Call
                    || (self.copy_readonly_calls
                        && matches!(function.instructions[id.as_raw() as usize].op,
                            Op::RuntimeCall | Op::ArrayPush | Op::MethodCall))
                {
                    let operands = function.instructions[id.as_raw() as usize].operands.clone();
                    for (position, source) in operands.into_iter().enumerate() {
                        if !self.scalar_values.contains(&source) {
                            continue;
                        }
                        let copy_id = InstId::from_raw(function.instructions.len() as u32);
                        let value = ValueId::from_raw(function.values.len() as u32);
                        function.values.push(Value {
                            ir_type: IrType::I64,
                            php_type: PhpType::Int,
                            ownership: Ownership::NonHeap,
                            def: ValueDef::Instruction {
                                block: function.blocks[block_index].id,
                                index: rewritten.len() as u32,
                                inst: copy_id,
                            },
                        });
                        function.instructions.push(Instruction::new(
                            Op::Move,
                            vec![source],
                            None,
                            Some(value),
                            IrType::I64,
                            PhpType::Int,
                            Ownership::NonHeap,
                            Op::Move.default_effects(),
                            None,
                        ));
                        rewritten.push(copy_id);
                        function.instructions[id.as_raw() as usize].operands[position] = value;
                    }
                }
                if self.updates.contains(&id) {
                    if function.instructions[id.as_raw() as usize].op == Op::StrIncDec {
                        let delta = match function.instructions[id.as_raw() as usize].immediate {
                            Some(Immediate::I64(delta)) => delta,
                            _ => unreachable!(),
                        };
                        let constant_id = InstId::from_raw(function.instructions.len() as u32);
                        let value = ValueId::from_raw(function.values.len() as u32);
                        function.values.push(Value {
                            ir_type: IrType::I64,
                            php_type: PhpType::Int,
                            ownership: Ownership::NonHeap,
                            def: ValueDef::Instruction {
                                block: function.blocks[block_index].id,
                                index: rewritten.len() as u32,
                                inst: constant_id,
                            },
                        });
                        function.instructions.push(Instruction::new(
                            Op::ConstI64,
                            vec![],
                            Some(Immediate::I64(delta)),
                            Some(value),
                            IrType::I64,
                            PhpType::Int,
                            Ownership::NonHeap,
                            Op::ConstI64.default_effects(),
                            None,
                        ));
                        rewritten.push(constant_id);
                        let inst = &mut function.instructions[id.as_raw() as usize];
                        inst.op = Op::ICheckedAddToInt;
                        inst.immediate = None;
                        inst.operands.push(value);
                        inst.effects = Op::ICheckedAddToInt.default_effects();
                    } else {
                        // A compound `$i += c` / `$i -= c` update already carries its constant
                        // operand; only the opcode has to become the checked integer form.
                        let inst = &mut function.instructions[id.as_raw() as usize];
                        let op = seed_op(inst);
                        if let Some(op) = op {
                            inst.op = op;
                            inst.immediate = None;
                            inst.effects = op.default_effects();
                        }
                    }
                }
                rewritten.push(id);
            }
            for (index, &id) in rewritten.iter().enumerate() {
                if let Some(value) = function.instructions[id.as_raw() as usize].result {
                    function.values[value.as_raw() as usize].def = ValueDef::Instruction {
                        block: function.blocks[block_index].id,
                        index: index as u32,
                        inst: id,
                    };
                }
            }
            function.blocks[block_index].instructions = rewritten;
        }
    }
}

/// Returns the instruction that defines `value`, or `None` for a block parameter.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some(inst)
}

/// Returns whether a main frame's private counter slots cannot be read by name elsewhere.
///
/// Top-level locals that another body reaches through `global` are `GlobalAlias` slots, which the
/// candidate filter excludes, and eval is rejected separately. A runtime `include` is the one
/// remaining way to hand the whole frame to other code, so main keeps boxed storage there.
pub(super) fn main_frame_is_private(function: &Function, data: &DataPool) -> bool {
    !function.instructions.iter().any(|inst| {
        inst.op == Op::RuntimeCall
            && matches!(inst.immediate, Some(Immediate::Data(id))
                if data.strings.get(id.as_raw() as usize)
                    .is_some_and(|label| label.starts_with("include ")))
    })
}

/// Collects compound `slot += constant` / `slot -= constant` update instructions.
fn compound_counter_updates(function: &Function, slots: &HashSet<LocalSlotId>) -> HashSet<InstId> {
    let mut updates = HashSet::new();
    for inst in &function.instructions {
        if inst.op != Op::StoreLocal {
            continue;
        }
        let Some(Immediate::LocalSlot(target)) = inst.immediate else {
            continue;
        };
        if !slots.contains(&target) {
            continue;
        }
        let Some(&stored) = inst.operands.first() else {
            continue;
        };
        if let Some(id) = counter_update_producer(function, target, stored) {
            updates.insert(id);
        }
    }
    updates
}

/// Follows ownership wrappers from a stored value to an increment or constant step.
fn stored_counter_update(function: &Function, slot: LocalSlotId, mut value: ValueId) -> bool {
    for _ in 0..8 {
        let Some(inst) = defining_instruction(function, value) else {
            return false;
        };
        match inst.op {
            Op::StrIncDec => return true,
            Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone | Op::EnsureOwned
                if inst.immediate.is_none() =>
            {
                let Some(&source) = inst.operands.first() else {
                    return false;
                };
                value = source;
            }
            _ => return counter_step(function, slot, inst),
        }
    }
    false
}

/// Follows ownership wrappers from a stored value back to a bounded counter step.
fn counter_update_producer(
    function: &Function,
    slot: LocalSlotId,
    mut value: ValueId,
) -> Option<InstId> {
    for _ in 0..8 {
        let id = defining_id(function, value)?;
        let inst = function.instruction(id)?;
        match inst.op {
            Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone | Op::EnsureOwned
                if inst.immediate.is_none() =>
            {
                value = *inst.operands.first()?;
            }
            _ => return counter_step(function, slot, inst).then_some(id),
        }
    }
    None
}

/// Requires `slot + constant` or `slot - constant` in any of its lowering opcodes.
///
/// The counter must be the left operand, so `1 - $i` is never read as a counter update.
pub(super) fn counter_step(function: &Function, slot: LocalSlotId, inst: &Instruction) -> bool {
    let add = matches!(
        (inst.op, &inst.immediate),
        (Op::ICheckedAdd | Op::ICheckedAddToInt, _)
            | (
                Op::MixedNumericBinop,
                Some(Immediate::MixedNumericOp(MixedNumericOp::Add))
            )
    );
    let sub = matches!(
        (inst.op, &inst.immediate),
        (Op::ICheckedSub | Op::ICheckedSubToInt, _)
            | (
                Op::MixedNumericBinop,
                Some(Immediate::MixedNumericOp(MixedNumericOp::Sub))
            )
    );
    if !(add || sub) || inst.operands.len() != 2 {
        return false;
    }
    let [counter, step] = inst.operands.as_slice() else {
        return false;
    };
    int_constant(function, *step).is_some() && loads_slot(function, slot, *counter)
}

/// Follows ownership wrappers to a plain load of `slot`.
fn loads_slot(function: &Function, slot: LocalSlotId, mut value: ValueId) -> bool {
    for _ in 0..8 {
        let Some(inst) = defining_instruction(function, value) else {
            return false;
        };
        match inst.op {
            Op::LoadLocal if inst.immediate == Some(Immediate::LocalSlot(slot)) => return true,
            Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone | Op::EnsureOwned
                if inst.immediate.is_none() =>
            {
                let Some(&source) = inst.operands.first() else {
                    return false;
                };
                value = source;
            }
            _ => return false,
        }
    }
    false
}

/// Returns the integer constant a value carries, through transparent owners.
fn int_constant(function: &Function, mut value: ValueId) -> Option<i64> {
    for _ in 0..8 {
        let inst = defining_instruction(function, value)?;
        if let (Op::ConstI64, Some(Immediate::I64(integer))) = (inst.op, &inst.immediate) {
            return Some(*integer);
        }
        if matches!(
            inst.op,
            Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone | Op::EnsureOwned
        ) && inst.immediate.is_none()
        {
            value = *inst.operands.first()?;
        } else {
            return None;
        }
    }
    None
}

/// Returns whether a typed runtime call accepts `value` as a concrete integer operand.
///
/// Only arithmetic operations whose integer lowering already exists are listed. Everything else
/// keeps the proven counter boxed, so an ABI that expects a `Mixed` cell is never handed raw
/// integer bits.
fn runtime_call_accepts_integer_counter(inst: &Instruction, value: ValueId) -> bool {
    use crate::ir::RuntimeCallTarget;
    let Some(Immediate::RuntimeCall(target)) = &inst.immediate else {
        return false;
    };
    let target = match target {
        RuntimeCallTarget::Function(target)
        | RuntimeCallTarget::ProfiledFunction { target, .. } => *target,
        _ => return false,
    };
    let allowed_position = match target {
        crate::ir::RuntimeFnId::Intdiv => 2,
        crate::ir::RuntimeFnId::Abs
        | crate::ir::RuntimeFnId::Sqrt
        | crate::ir::RuntimeFnId::Round => 1,
        // `sprintf` keeps the literal format at position zero and packs every following value
        // into a 16-byte record; the packing lowering handles a raw integer operand.
        crate::ir::RuntimeFnId::Sprintf => usize::MAX,
        _ => return false,
    };
    inst.operands.iter().enumerate().all(|(index, operand)| {
        *operand != value || (index < allowed_position && (target != crate::ir::RuntimeFnId::Sprintf || index >= 1))
    })
}

/// Maps a counter seed producer to the checked integer opcode its trial must prove unchecked.
fn seed_op(inst: &Instruction) -> Option<Op> {
    match (inst.op, &inst.immediate) {
        (Op::ICheckedAdd, _)
        | (Op::MixedNumericBinop, Some(Immediate::MixedNumericOp(MixedNumericOp::Add))) => {
            Some(Op::ICheckedAddToInt)
        }
        (Op::ICheckedSub, _)
        | (Op::MixedNumericBinop, Some(Immediate::MixedNumericOp(MixedNumericOp::Sub))) => {
            Some(Op::ICheckedSubToInt)
        }
        (Op::ICheckedMul, _)
        | (Op::MixedNumericBinop, Some(Immediate::MixedNumericOp(MixedNumericOp::Mul))) => {
            Some(Op::ICheckedMulToInt)
        }
        _ => None,
    }
}

/// Accepts a checked `+` or `-` of two raw integers as a bound a counter may be compared with.
///
/// The bound may still overflow to a float. That is why the comparison keeps its PHP opcode and
/// why IntegerRange, not this check, decides whether the bound limits the counter.
fn checked_bound(function: &Function, value: ValueId) -> bool {
    defining_instruction(function, value).is_some_and(|inst| {
        matches!(inst.op, Op::ICheckedAdd | Op::ICheckedSub)
            && inst.operands.len() == 2
            && inst.operands.iter().all(|&operand| integer(function, operand))
    })
}

/// Returns whether a conversion only reads the proven integer bits.
///
/// Int, float and string targets all consume the payload without taking ownership of the source
/// cell, and the range proof makes the conversion value-identical to the boxed original.
fn observational_cast(inst: &Instruction) -> bool {
    inst.op == Op::Cast
        && matches!(
            (&inst.immediate, &inst.result_php_type),
            (Some(Immediate::CastTarget(IrType::I64)), PhpType::Int)
                | (Some(Immediate::CastTarget(IrType::F64)), PhpType::Float)
                | (Some(Immediate::CastTarget(IrType::Str)), PhpType::Str)
        )
}

/// Accepts an already raw PHP integer without assuming anything about a Mixed label.
fn integer(function: &Function, value: ValueId) -> bool {
    function
        .value(value)
        .is_some_and(|value| value.ir_type == IrType::I64 && value.php_type == PhpType::Int)
        && defining_instruction(function, value).is_none_or(|inst| inst.op != Op::Nop)
}
