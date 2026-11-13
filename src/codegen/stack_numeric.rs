//! Purpose:
//! Proves which checked numeric results may use a frame-local Mixed cell.
//!
//! Called from:
//! - Frame layout before any generated instruction is emitted.
//!
//! Key details:
//! - Numeric consumers copy payloads; one local or owned operand slot may retain the pointer,
//!   beside any block-scoped call-operand temporaries that pin it while an operand is evaluated.
//! - A string cast is a payload observation: numeric tags render into concat scratch without
//!   adopting the cell, so an accumulator echoed at loop exit keeps its frame storage.
//! - Pointer reads cannot survive another execution of the producing instruction.
//! - Independent producers may intervene, but owning-slot replacements invalidate borrowed reads.
//! - Array and hash keys only copy the cell into key registers, so they are numeric observations.
//! - References, eval, suspension, unknown consumers and escaping values fail closed.
//! - A raw integer/float seed stored into a proven numeric local gets one cell per slot; the
//!   slot exclusively owns that cell, so only a later store to the same slot rewrites it.
//! - An explicit box of a raw integer/float stored into one such local is a cell producer too.
//! - A Mixed clone of a proven numeric value copies only its tag and payload, so reading a cell
//!   into a clone is an observation, and the clone itself can own a frame cell for one local
//!   (an update's key snapshot, say) instead of a fresh heap box.

use crate::ir::{
    Function, Immediate, InstId, IrHeapKind, IrType, LocalKind, LocalSlotId, Op, Ownership,
    ValueDef, ValueId,
};
use crate::types::PhpType;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod comparison_tests;
mod placement;
mod retirement;
pub(super) use retirement::frame_only_locals;
use crate::ir_passes::numeric_domain as domain;

/// Stack cells and the exclusively numeric locals proven while selecting them.
#[derive(Default)]
pub(super) struct NumericFrame {
    #[cfg(test)]
    pub(super) cells: Vec<ValueId>,
    pub(super) cell_groups: Vec<Vec<ValueId>>,
    /// Proven numeric locals whose raw scalar seed stores write a dedicated frame cell.
    pub(super) seed_slots: Vec<LocalSlotId>,
    pub(super) locals: HashSet<LocalSlotId>,
    pub(super) proven_values: HashSet<ValueId>,
}

/// The frame cells and proven numeric locals of one function, as codegen would select them.
///
/// An optimizer pass compares this proof before and after a rewrite, so a rewrite that
/// moves a read away from its user cannot send another local back to heap boxes.
pub(crate) struct FrameCells {
    locals: HashSet<LocalSlotId>,
    values: HashSet<ValueId>,
}

impl FrameCells {
    /// Runs the frame-cell proof on `function`.
    pub(crate) fn of(function: &Function) -> Self {
        let frame = plan(function);
        Self {
            locals: frame.locals,
            values: frame.cell_groups.into_iter().flatten().collect(),
        }
    }

    /// Returns whether every access to the slot passed the numeric payload-copy proof.
    #[cfg(test)]
    pub(crate) fn contains_local(&self, slot: LocalSlotId) -> bool {
        self.locals.contains(&slot)
    }

    /// Returns true when `after` keeps every cell and numeric local of this proof.
    ///
    /// `slots` and `values` are the locals and results the rewrite replaced on purpose.
    pub(crate) fn kept_by(
        &self,
        after: &Self,
        slots: &HashSet<LocalSlotId>,
        values: &HashSet<ValueId>,
    ) -> bool {
        self.locals.iter().all(|slot| slots.contains(slot) || after.locals.contains(slot))
            && self.values.iter().all(|value| values.contains(value) || after.values.contains(value))
    }
}

/// Returns checked producers whose complete pointer lifetime stays inside their frame.
pub(super) fn plan(function: &Function) -> NumericFrame {
    if !function.instructions.iter().any(producer_op) {
        return NumericFrame::default();
    }
    if function.flags.is_generator
        || function.flags.is_fiber_wrapper
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
        return NumericFrame::default();
    }
    let mut uses: HashMap<ValueId, Vec<InstId>> = HashMap::new();
    let mut terminator_uses = HashSet::new();
    let mut locations = HashMap::new();
    let mut accesses: HashMap<LocalSlotId, Vec<InstId>> = HashMap::new();
    for block in &function.blocks {
        for (index, &id) in block.instructions.iter().enumerate() {
            let inst = &function.instructions[id.as_raw() as usize];
            locations.insert(id, (block.id, index));
            match &inst.immediate {
                Some(Immediate::LocalSlot(slot)) => accesses.entry(*slot).or_default().push(id),
                Some(Immediate::LocalSlotPair { first, second }) => {
                    accesses.entry(*first).or_default().push(id);
                    accesses.entry(*second).or_default().push(id);
                }
                Some(Immediate::IterStart(iter)) => {
                    for slot in iter.local_slots() {
                        accesses.entry(slot).or_default().push(id);
                    }
                }
                _ => {}
            }
            for &value in &function.instructions[id.as_raw() as usize].operands {
                uses.entry(value).or_default().push(id);
            }
        }
        if let Some(term) = &block.terminator {
            terminator_uses.extend(term.used_values());
        }
    }
    let mut proof = Proof {
        function,
        domain: RefCell::new(domain::NumericInputs::new(function)),
        uses,
        terminator_uses,
        locations,
        accesses,
        slots: HashMap::new(),
        invalidations: RefCell::new(HashMap::new()),
    };
    // Collected first: `cell_box` borrows `proof` shared, while `producer` below needs it `&mut`.
    let candidates: Vec<_> = function
        .instructions
        .iter()
        .filter(|inst| producer_op(inst) || proof.cell_box(inst))
        .filter_map(|inst| inst.result)
        .collect();
    let cells: Vec<_> = candidates
        .into_iter()
        .filter(|&value| proof.producer(value))
        .collect();
    let cell_groups = placement::storage_groups(&proof, &cells);
    let seed_slots = seed_slots(&mut proof);
    NumericFrame {
        #[cfg(test)]
        cells,
        cell_groups,
        seed_slots,
        proven_values: proof.domain.into_inner().into_proven_values(),
        locals: proof
            .slots
            .into_iter()
            .filter_map(|(slot, safe)| safe.then_some(slot))
            .collect(),
    }
}

/// Selects proven numeric locals written by raw scalar seeds, in slot order.
///
/// Every other access to such a slot already passed `Proof::slot`: loads copy payloads before
/// any write to the slot, and no pointer read from it reaches another owner. A seed store
/// therefore rewrites the slot's own cell only while replacing the slot's occupant.
fn seed_slots(proof: &mut Proof<'_>) -> Vec<LocalSlotId> {
    let mut seeded: Vec<_> = proof
        .function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::StoreLocal)
        .filter_map(|inst| {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                return None;
            };
            let value = proof.function.value(*inst.operands.first()?)?;
            scalar_numeric(&value.ir_type, &value.php_type).then_some(slot)
        })
        .collect();
    seeded.sort_unstable_by_key(|slot| slot.as_raw());
    seeded.dedup();
    seeded.retain(|&slot| proof.slot(slot));
    seeded
}

/// Recognizes the raw integer and float representations a seed cell can tag.
pub(super) fn scalar_numeric(ir_type: &IrType, php_type: &PhpType) -> bool {
    matches!(
        (ir_type, php_type),
        (IrType::I64, PhpType::Int) | (IrType::F64, PhpType::Float)
    )
}

/// Shared use graph and cached whole-slot proofs for one function.
struct Proof<'a> {
    function: &'a Function,
    domain: RefCell<domain::NumericInputs<'a>>,
    uses: HashMap<ValueId, Vec<InstId>>,
    terminator_uses: HashSet<ValueId>,
    slots: HashMap<LocalSlotId, bool>,
    locations: HashMap<InstId, (crate::ir::BlockId, usize)>,
    accesses: HashMap<LocalSlotId, Vec<InstId>>,
    invalidations: RefCell<HashMap<LocalSlotId, HashMap<crate::ir::BlockId, Vec<usize>>>>,
}

impl Proof<'_> {
    /// Reuses only completed numeric facts while the function remains immutably borrowed.
    fn numeric_inputs(&self, values: &[ValueId]) -> bool {
        self.domain.borrow_mut().prove(values)
    }

    /// Recognizes a Mixed clone whose whole input graph holds only integers or floats.
    ///
    /// `__rt_mixed_clone` of such a value only copies the tag and payload words into a fresh
    /// box, so a frame cell holding the same words is an equivalent private copy.
    fn clone_box(&self, inst: &crate::ir::Instruction) -> bool {
        inst.op == Op::MixedClone
            && inst.operands.len() == 1
            && inst.result_type == IrType::Heap(IrHeapKind::Mixed)
            && self.numeric_inputs(&inst.operands)
    }

    /// Recognizes every box that can be written straight into one local's frame cell.
    fn cell_box(&self, inst: &crate::ir::Instruction) -> bool {
        numeric_box(self.function, inst) || self.clone_box(inst)
    }

    /// Requires an owned numeric producer with an independently proven scalar-input ABI.
    fn producer(&mut self, value: ValueId) -> bool {
        let Some((id, definition)) = definition(self.function, value) else {
            return false;
        };
        let boxed = self.cell_box(definition);
        // A scalar box is always fresh; guarded exits label it with the generic Mixed ownership.
        let owned = definition.result_ownership == Ownership::Owned
            || (boxed && definition.result_ownership == Ownership::MaybeOwned);
        if definition.result_type != IrType::Heap(IrHeapKind::Mixed)
            || definition.result_php_type != PhpType::Mixed
            || !owned
        {
            return false;
        }
        if !boxed && (!producer_op(definition)
            || definition.operands.len() != if domain::increment(definition) { 1 } else { 2 }
            || (!tagged_producer(definition) && definition.operands.iter().any(|&input| {
                self.function.value(input).is_none_or(|input| {
                    input.ir_type != IrType::I64 || input.php_type != PhpType::Int
                })
            }))
            || (tagged_producer(definition) && !self.numeric_inputs(&definition.operands)))
        {
            return false;
        }
        let mut stores = HashSet::new();
        if !self.value_uses(value, id, &mut HashSet::new(), &mut stores)
            || stores.iter().filter(|&&slot| !self.scoped_temp(slot)).count() > 1
        {
            return false;
        }
        // A box is selected only as a local seed (a guarded counter's exit publication).
        if boxed && stores.len() != 1 {
            return false;
        }
        stores.into_iter().all(|slot| self.slot(slot))
    }

    /// Allows only same-block numeric observations, transparent retains/borrows and one destination.
    fn value_uses(
        &self,
        value: ValueId,
        producer: InstId,
        seen: &mut HashSet<ValueId>,
        stores: &mut HashSet<LocalSlotId>,
    ) -> bool {
        if !seen.insert(value) {
            return true;
        }
        if self.terminator_uses.contains(&value) {
            return false;
        }
        self.uses.get(&value).is_some_and(|uses| {
            uses.iter().all(|&id| {
                if self
                    .locations
                    .get(&producer)
                    .zip(self.locations.get(&id))
                    .is_none_or(|(producer, user)| producer.0 != user.0)
                {
                    return false;
                }
                let user = &self.function.instructions[id.as_raw() as usize];
                match user.op {
                    Op::PropertyBoundaryCheckedToInt => int_property_observation(user),
                    Op::Release => true,
                    Op::Acquire | Op::Borrow if user.immediate.is_none() => user
                        .result
                        .is_some_and(|result| self.value_uses(result, producer, seen, stores)),
                    Op::Cast | Op::ReturnBoundaryMixedToInt => scalar_observation(user),
                    _ if key_observation(user, value) => true,
                    _ if (domain::comparison(user) || domain::identity(user))
                        && self.numeric_inputs(&user.operands) => true,
                    _ if tagged_producer(user) && self.numeric_inputs(&user.operands) => true,
                    Op::StoreLocal => match user.immediate {
                        Some(Immediate::LocalSlot(slot)) => {
                            stores.insert(slot);
                            true
                        }
                        _ => false,
                    },
                    _ => false,
                }
            })
        })
    }

    /// Checks every access before admitting a pointer into a PHP local across a backedge.
    fn slot(&mut self, slot: LocalSlotId) -> bool {
        if let Some(&safe) = self.slots.get(&slot) {
            return safe;
        }
        let Some(local) = self.function.locals.get(slot.as_raw() as usize) else {
            return false;
        };
        let scoped = matches!(local.kind, LocalKind::OwnedTemp | LocalKind::HiddenTemp)
            && self.scoped_operand(slot);
        if (slot.as_raw() as usize) < self.function.params.len()
            || (local.kind != LocalKind::PhpLocal && !scoped)
            || self.function.no_epilogue_cleanup_slots.contains(&slot)
            || local.php_type.codegen_repr() != PhpType::Mixed
            || local.ir_type != IrType::Heap(IrHeapKind::Mixed)
        {
            return false;
        }
        let safe = self.accesses.get(&slot).is_some_and(|accesses| {
            accesses.iter().all(|&id| {
                let inst = &self.function.instructions[id.as_raw() as usize];
                match inst.op {
                    Op::StoreLocal => inst
                        .operands
                        .first()
                        .is_some_and(|&value| self.slot_writer(value, slot)),
                    Op::LoadLocal => inst.result.is_some_and(|value| {
                        let Some(metadata) = self.function.value(value) else {
                            return false;
                        };
                        if matches!(
                            (&metadata.ir_type, &metadata.php_type),
                            (IrType::I64, PhpType::Int) | (IrType::F64, PhpType::Float)
                        ) {
                            return true;
                        }
                        self.load_uses(id, value)
                    }),
                    Op::ReleaseLocalSlot => true,
                    Op::PushCallOperandOwner | Op::PopCallOperandOwner | Op::UnsetLocal => {
                        scoped && inst.operands.is_empty()
                    }
                    _ => false,
                }
            })
        });
        self.slots.insert(slot, safe);
        safe
    }

    /// Returns whether `slot` is a call-operand temporary scoped to one block.
    ///
    /// Such a slot pins the cell only while an operand is evaluated and is retired in the same
    /// block before the producer can run again, so several of them may hold one cell beside its
    /// single ordinary destination.
    fn scoped_temp(&self, slot: LocalSlotId) -> bool {
        self.function.locals.get(slot.as_raw() as usize).is_some_and(|local| {
            matches!(local.kind, LocalKind::OwnedTemp | LocalKind::HiddenTemp)
        }) && self.scoped_operand(slot)
    }

    /// Requires one same-block stored operand with balanced publication and final retirement.
    fn scoped_operand(&self, slot: LocalSlotId) -> bool {
        let Some(accesses) = self.accesses.get(&slot) else { return false; };
        let mut block = None;
        let (mut depth, mut stored, mut published, mut retired) = (0usize, false, false, false);
        for &id in accesses {
            let Some(&(current, _)) = self.locations.get(&id) else { return false; };
            if block.is_some_and(|block| block != current) { return false; }
            block = Some(current);
            match self.function.instructions[id.as_raw() as usize].op {
                Op::StoreLocal => {
                    if stored { return false; }
                    stored = true;
                }
                Op::PushCallOperandOwner => {
                    if !stored || retired { return false; }
                    depth += 1; published = true;
                }
                Op::PopCallOperandOwner => {
                    let Some(next) = depth.checked_sub(1) else { return false; };
                    depth = next;
                }
                // An operand temporary's `unset_local` only zeroes the slot; the value's own
                // `release` retires it, exactly like a `release_local_slot` after the scope.
                Op::ReleaseLocalSlot | Op::UnsetLocal if stored => {
                    if depth != 0 { return false; }
                    retired = true;
                }
                _ => {}
            }
        }
        stored && published && retired && depth == 0
    }

    /// Allows fresh numeric seed cells and producers that cannot feed any other local.
    fn slot_writer(&self, mut value: ValueId, slot: LocalSlotId) -> bool {
        for _ in 0..self.function.values.len() {
            let Some(metadata) = self.function.value(value) else {
                return false;
            };
            if matches!(
                (&metadata.ir_type, &metadata.php_type),
                (IrType::I64, PhpType::Int) | (IrType::F64, PhpType::Float)
            ) {
                return true;
            }
            let Some((id, inst)) = definition(self.function, value) else {
                return false;
            };
            match inst.op {
                Op::Acquire if numeric_box(self.function, inst) => return true,
                Op::Acquire if inst.immediate.is_none() => {
                    let Some(&source) = inst.operands.first() else {
                        return false;
                    };
                    value = source;
                }
                Op::MixedClone => return self.clone_box(inst),
                Op::MixedBox => {
                    return inst.operands.first().is_some_and(|&source| {
                        self.function.value(source).is_some_and(|source| {
                            matches!(
                                (&source.ir_type, &source.php_type),
                                (IrType::I64, PhpType::Int) | (IrType::F64, PhpType::Float)
                            )
                        })
                    })
                }
                _ if producer_op(inst) => {
                    let mut stores = HashSet::new();
                    return (!tagged_producer(inst) || self.numeric_inputs(&inst.operands))
                        && self.value_uses(value, id, &mut HashSet::new(), &mut stores)
                        && stores.contains(&slot)
                        && stores.iter().all(|&other| other == slot || self.scoped_temp(other));
                }
                _ => return false,
            }
        }
        false
    }

    /// A borrowed pointer must be converted before its frame cell can be written again.
    fn load_uses(&self, load: InstId, value: ValueId) -> bool {
        if self.terminator_uses.contains(&value) {
            return false;
        }
        self.uses.get(&value).is_none_or(|uses| {
            uses.iter().all(|&user| {
                let inst = &self.function.instructions[user.as_raw() as usize];
                if int_property_observation(inst) {
                    return self.stable_read(load, user);
                }
                if inst.op == Op::Release {
                    return true;
                }
                // Strict identity copies the tag and payload like an ordered comparison.
                if domain::identity(inst) && self.numeric_inputs(&inst.operands) {
                    return self.stable_read(load, user);
                }
                // A numeric clone copies the tag and payload into its own cell or heap box.
                if self.clone_box(inst) {
                    return self.stable_read(load, user);
                }
                let arithmetic = tagged_producer(inst) && self.numeric_inputs(&inst.operands);
                let comparison = domain::comparison(inst) && self.numeric_inputs(&inst.operands);
                if !scalar_observation(inst)
                    && !tag_or_payload_copy(inst)
                    && !arithmetic
                    && !comparison
                    && !key_observation(inst, value)
                {
                    return false;
                }
                self.stable_read(load, user)
            })
        })
    }

    /// Allows intervening independent cells while rejecting any possible overwrite of this read.
    fn stable_read(&self, load: InstId, user: InstId) -> bool {
        let Some(Immediate::LocalSlot(slot)) = self.function.instructions[load.as_raw() as usize].immediate else {
            return false;
        };
        let Some((start, end)) = self.locations.get(&load).zip(self.locations.get(&user)) else {
            return false;
        };
        if start.0 != end.0 || start.1 >= end.1 { return false; }
        let mut cache = self.invalidations.borrow_mut();
        let writes = cache.entry(slot).or_insert_with(|| self.slot_invalidations(slot));
        writes.get(&start.0).is_none_or(|positions| {
            let next = positions.partition_point(|position| *position <= start.1);
            positions.get(next).is_none_or(|position| *position >= end.1)
        })
    }

    /// Indexes slot retirement/stores and every producer that can back the slot on any path.
    fn slot_invalidations(&self, slot: LocalSlotId) -> HashMap<crate::ir::BlockId, Vec<usize>> {
        let mut invalidating = HashSet::new();
        for &id in self.accesses.get(&slot).into_iter().flatten() {
            let inst = &self.function.instructions[id.as_raw() as usize];
            if !matches!(inst.op, Op::StoreLocal | Op::ReleaseLocalSlot) { continue; }
            invalidating.insert(id);
            if inst.op != Op::StoreLocal { continue; }
            let Some(&source) = inst.operands.first() else { continue; };
            let mut source = source;
            for _ in 0..self.function.values.len() {
                let Some((producer, writer)) = definition(self.function, source) else { break; };
                if producer_op(writer) || self.cell_box(writer) { invalidating.insert(producer); break; }
                if writer.op != Op::Acquire || writer.immediate.is_some() { break; }
                let Some(&input) = writer.operands.first() else { break; };
                source = input;
            }
        }
        let mut writes: HashMap<_, Vec<_>> = HashMap::new();
        for id in invalidating {
            if let Some(&(block, position)) = self.locations.get(&id) {
                writes.entry(block).or_default().push(position);
            }
        }
        for positions in writes.values_mut() { positions.sort_unstable(); positions.dedup(); }
        writes
    }
}

/// Identifies operations whose result owns no heap children, even after overflow.
fn checked(op: Op) -> bool {
    matches!(op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)
}

/// Recognizes a Mixed box of a raw integer or float, such as a guarded exit seed.
///
/// An `acquire` boxes too when range analysis has narrowed the checked producer it retained to a
/// raw scalar (`$j = $i - 1` into an `int|float` local): its lowering allocates the same cell.
pub(super) fn numeric_box(function: &Function, inst: &crate::ir::Instruction) -> bool {
    (inst.op == Op::MixedBox
        || (inst.op == Op::Acquire
            && inst.immediate.is_none()
            && inst.result_type == IrType::Heap(IrHeapKind::Mixed)))
        && inst.operands.len() == 1
        && function
            .value(inst.operands[0])
            .is_some_and(|source| scalar_numeric(&source.ir_type, &source.php_type))
}

/// Includes tagged arithmetic only when its emitter preserves concrete numeric tags.
fn producer_op(inst: &crate::ir::Instruction) -> bool {
    checked(inst.op) || tagged_producer(inst)
}

/// Includes unary updates only when their complete input graph is numeric.
fn tagged_producer(inst: &crate::ir::Instruction) -> bool {
    domain::mixed(inst) || domain::increment(inst)
}

/// Shares the closed numeric proof with comparison lowering; no pointer lifetime is extended.
pub(super) fn numeric_comparison(function: &Function, proven: &HashSet<ValueId>, inst: &crate::ir::Instruction) -> bool {
    domain::comparison(inst) && inst.operands.len() == 2
        && (inst.operands.iter().all(|value| proven.contains(value))
            || domain::inputs(function, &inst.operands))
}

/// Admits strict identity only when both operands can hold just integer or float tags.
pub(super) fn numeric_identity(function: &Function, proven: &HashSet<ValueId>, inst: &crate::ir::Instruction) -> bool {
    domain::identity(inst)
        && (inst.operands.iter().all(|value| proven.contains(value))
            || domain::inputs(function, &inst.operands))
}

/// Shares the complete scalar-origin proof with retirement, without admitting any pointer escape.
pub(super) fn numeric_value(function: &Function, proven: &HashSet<ValueId>, value: ValueId) -> bool {
    proven.contains(&value) || domain::inputs(function, &[value])
}

/// Reads a numeric tag or copies an integer payload, as a guarded counter entry does.
fn tag_or_payload_copy(inst: &crate::ir::Instruction) -> bool {
    use crate::ir::PhpTypePredicate;
    inst.operands.len() == 1
        && match inst.op {
            Op::TypePredicate => matches!(
                inst.immediate,
                Some(Immediate::TypePredicate(PhpTypePredicate::Int | PhpTypePredicate::Float))
            ),
            Op::MixedUnbox => inst.result_type == IrType::I64 && inst.result_php_type == PhpType::Int,
            _ => false,
        }
}

/// Copies a scalar payload without exposing its address; declared returns retain their checks.
///
/// A string cast is an observation, not an escape: `__rt_mixed_cast_string` reads the tag and
/// payload and formats numeric tags into concat scratch, so it never retains or frees the cell.
/// The numeric-domain proof guarantees every value reaching this cell is an int or float, the
/// two tags that format without adopting the source storage.
fn scalar_observation(inst: &crate::ir::Instruction) -> bool {
    (inst.op == Op::ReturnBoundaryMixedToInt
        && inst.operands.len() == 1
        && inst.result_type == IrType::I64 && inst.result_php_type == PhpType::Int)
        || (inst.op == Op::Cast
        && matches!(
            (&inst.immediate, &inst.result_php_type),
            (Some(Immediate::CastTarget(IrType::I64)), PhpType::Int)
                | (Some(Immediate::CastTarget(IrType::F64)), PhpType::Float)
                | (Some(Immediate::CastTarget(IrType::Str)), PhpType::Str)
        ))
}

/// Copies a checked payload into a declared `int` property word; the narrowing keeps PHP's
/// typed-property TypeError and never retains or frees the cell.
fn int_property_observation(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::PropertyBoundaryCheckedToInt
        && inst.operands.len() == 1
        && inst.result_type == IrType::I64
        && inst.result_php_type == PhpType::Int
}

/// Accepts a cell read only as the key of an array or hash access.
///
/// Every key materializer (`materialize_hash_key_*`, inline for a proven numeric cell and
/// through `__rt_mixed_unbox` otherwise) copies the cell's tag and payload into key registers
/// before the access runs; `__rt_array_set_mixed_key` only unboxes its borrowed key cell. None
/// retains the cell, so the pointer cannot outlive the instruction. The value must not also be the stored
/// element or the receiver, which would publish the frame address into the container.
fn key_observation(inst: &crate::ir::Instruction, value: ValueId) -> bool {
    matches!(
        inst.op,
        Op::ArrayGet | Op::ArrayGetSilent | Op::ArraySet | Op::HashGet | Op::HashGetSilent | Op::HashSet
            | Op::ArrayGetMixedKey | Op::ArrayGetMixedKeySilent | Op::ArraySetMixedKey
    ) && inst.operands.get(1) == Some(&value)
        && inst.operands.iter().enumerate().all(|(position, &operand)| position == 1 || operand != value)
}

/// Finds the defining instruction for a non-parameter SSA value.
fn definition(function: &Function, value: ValueId) -> Option<(InstId, &crate::ir::Instruction)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Recognizes only transparent retains of a selected stack cell.
pub(super) fn is_stack_value(
    function: &Function,
    cells: &HashMap<ValueId, usize>,
    mut value: ValueId,
) -> bool {
    for _ in 0..function.values.len() {
        if cells.contains_key(&value) {
            return true;
        }
        let Some((_, inst)) = definition(function, value) else {
            return false;
        };
        if !matches!(inst.op, Op::Acquire | Op::Borrow) || inst.immediate.is_some() {
            return false;
        }
        let Some(&source) = inst.operands.first() else {
            return false;
        };
        value = source;
    }
    false
}

/// Finds a borrowed read of an exclusively numeric local for direct scalar cleanup.
pub(super) fn numeric_local_read(
    function: &Function,
    locals: &HashSet<LocalSlotId>,
    value: ValueId,
) -> bool {
    let Some((_, inst)) = definition(function, value) else {
        return false;
    };
    inst.op == Op::LoadLocal
        && matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if locals.contains(&slot))
}
