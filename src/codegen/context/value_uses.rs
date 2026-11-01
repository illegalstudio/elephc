//! Purpose:
//! Indexes, once per emitted body, the instructions that ownership queries used to rescan.
//!
//! Called from:
//! - `super::FunctionContext` release and transfer predicates, and `super::operand_owners`.
//!
//! Key details:
//! - Every "is this value released / stored into a call-operand slot" question was a walk over the
//!   whole body, asked once per value: quadratic in body size. Laravel's largest bodies made those
//!   walks a quarter of the codegen phase.
//! - The body is immutable while it is emitted, so the index is built on first use and never
//!   invalidated. Answers are exactly the ones the walks gave.

use crate::fast_hash::{FastMap, FastSet};
use crate::ir::{Function, Immediate, LocalSlotId, Op, ValueId};

/// Positions of the ownership-relevant instructions of one body, by value and by slot.
#[derive(Default)]
pub(in crate::codegen) struct ValueUseIndex {
    /// Index of the LAST `Release` whose first operand is the value.
    last_release: FastMap<ValueId, u32>,
    /// `StoreLocal value -> slot` instructions, in body order.
    stores: FastMap<ValueId, Vec<(u32, LocalSlotId)>>,
    /// Slots some `PushCallOperandOwner` publishes.
    published_slots: FastSet<LocalSlotId>,
    /// Index of the LAST `ReleaseLocalSlot` of each slot.
    last_slot_release: FastMap<LocalSlotId, u32>,
    /// Values a terminator or a storing/absorbing instruction takes over: a `return`, a branch
    /// argument, a store into a slot, property, array or box. Such a value must own its payload.
    consumed: FastSet<ValueId>,
    /// `consumed` minus the values only a `MixedBox` takes. `lower_mixed_box` always RETAINS its
    /// operand into a cell of its own, so a box leaves the operand's ownership where it was.
    consumed_beyond_box: FastSet<ValueId>,
    /// `ArrayPush` instructions that append to an `array_new` of their own block that no other
    /// instruction has used yet: an array literal under construction, still packed storage.
    fresh_literal_pushes: FastSet<u32>,
}

impl ValueUseIndex {
    /// Indexes one body in a single pass.
    pub(super) fn build(function: &Function) -> Self {
        let mut index = Self::default();
        for block in &function.blocks {
            if let Some(terminator) = &block.terminator {
                index.consumed.extend(terminator_operands(terminator));
                index.consumed_beyond_box.extend(terminator_operands(terminator));
            }
        }
        for (position, inst) in function.instructions.iter().enumerate() {
            if op_takes_over_operands(inst.op) {
                index.consumed.extend(inst.operands.iter().copied());
                if inst.op != Op::MixedBox {
                    index.consumed_beyond_box.extend(inst.operands.iter().copied());
                }
            }
            let position = position as u32;
            match (inst.op, inst.immediate.as_ref()) {
                (Op::Release, _) => {
                    if let Some(value) = inst.operands.first() {
                        index.last_release.insert(*value, position);
                    }
                }
                (Op::StoreLocal, Some(Immediate::LocalSlot(slot))) if inst.operands.len() == 1 => {
                    index
                        .stores
                        .entry(inst.operands[0])
                        .or_default()
                        .push((position, *slot));
                }
                (Op::PushCallOperandOwner, Some(Immediate::LocalSlot(slot))) => {
                    index.published_slots.insert(*slot);
                }
                (Op::ReleaseLocalSlot, Some(Immediate::LocalSlot(slot))) => {
                    index.last_slot_release.insert(*slot, position);
                }
                _ => {}
            }
        }
        index.fresh_literal_pushes = fresh_literal_pushes(function);
        index
    }

    /// Returns whether the instruction at `position` appends to an array literal under
    /// construction (see `fresh_literal_pushes`).
    pub(in crate::codegen) fn is_fresh_literal_push(&self, position: Option<u32>) -> bool {
        position.is_some_and(|position| self.fresh_literal_pushes.contains(&position))
    }

    /// Returns whether a terminator or a storing instruction takes `value` over.
    pub(in crate::codegen) fn is_consumed(&self, value: ValueId) -> bool {
        self.consumed.contains(&value)
    }

    /// Returns whether something other than a retaining `MixedBox` takes `value` over.
    pub(in crate::codegen) fn is_consumed_beyond_box(&self, value: ValueId) -> bool {
        self.consumed_beyond_box.contains(&value)
    }

    /// Returns whether any `Release` in the body drops `value`.
    pub(in crate::codegen) fn is_released(&self, value: ValueId) -> bool {
        self.last_release.contains_key(&value)
    }

    /// Returns whether a `Release` of `value` sits after instruction `position`.
    pub(in crate::codegen) fn is_released_after(&self, value: ValueId, position: Option<u32>) -> bool {
        self.last_release
            .get(&value)
            .is_some_and(|last| position.is_none_or(|position| *last > position))
    }

    /// Returns whether a scoped caller slot will retire this SSA owner's reference.
    ///
    /// The value is stored (other than at `current`) into a slot that a call publishes as an
    /// operand owner, and that slot is retired by a `ReleaseLocalSlot` after the store.
    pub(in crate::codegen) fn has_scoped_cleanup(&self, value: ValueId, current: Option<u32>) -> bool {
        let Some(stores) = self.stores.get(&value) else {
            return false;
        };
        stores.iter().any(|(store, slot)| {
            Some(*store) != current
                && self.published_slots.contains(slot)
                && self
                    .last_slot_release
                    .get(slot)
                    .is_some_and(|release| release > store)
        })
    }
}

/// Walks every block in order and keeps the `ArrayPush` instructions whose receiver is an
/// `array_new` of the same block that nothing but earlier pushes has touched. Any other use of the
/// array (a store, a key write, a call) ends its run, so a promotion to hash storage can never
/// precede a push this set holds.
fn fresh_literal_pushes(function: &Function) -> FastSet<u32> {
    let mut fresh = FastSet::default();
    let mut building: FastSet<ValueId> = FastSet::default();
    for block in &function.blocks {
        building.clear();
        for inst_id in &block.instructions {
            let Some(inst) = function.instruction(*inst_id) else {
                continue;
            };
            let mut operands = inst.operands.iter();
            if inst.op == Op::ArrayPush {
                if let Some(array) = operands.next() {
                    if building.contains(array) {
                        fresh.insert(inst_id.as_raw());
                    }
                }
            }
            for operand in operands {
                building.remove(operand);
            }
            if inst.op == Op::ArrayNew {
                if let Some(result) = inst.result {
                    building.insert(result);
                }
            }
        }
    }
    fresh
}

/// Instructions that keep (store, append, box) an operand rather than only reading it.
fn op_takes_over_operands(op: Op) -> bool {
    matches!(
        op,
        Op::StoreLocal
            | Op::StoreRefCell
            | Op::StoreGlobal
            | Op::StoreStaticLocal
            | Op::StoreStaticProperty
            | Op::StoreDynamicStaticProperty
            | Op::StoreReflectionStaticProperty
            | Op::MixedBox
            | Op::ArraySet
            | Op::HashSet
            | Op::DescriptorArgSet
            | Op::ArrayPush
            | Op::MixedArrayAppend
            | Op::HashAppend
            | Op::PropSet
            | Op::DynamicPropSet
            | Op::EvalScopeSet
            | Op::PackedFieldSet
    )
}

/// Lists every value a terminator reads: its condition, branch arguments, returned or thrown value.
fn terminator_operands(terminator: &crate::ir::Terminator) -> Vec<ValueId> {
    use crate::ir::Terminator;
    match terminator {
        Terminator::Br { args, .. } => args.clone(),
        Terminator::CondBr { cond, then_args, else_args, .. } => std::iter::once(*cond)
            .chain(then_args.iter().copied())
            .chain(else_args.iter().copied())
            .collect(),
        Terminator::Switch { scrutinee, cases, default_args, .. } => std::iter::once(*scrutinee)
            .chain(cases.iter().flat_map(|case| case.args.iter().copied()))
            .chain(default_args.iter().copied())
            .collect(),
        Terminator::Return { value } => value.iter().copied().collect(),
        Terminator::Throw { value } => vec![*value],
        _ => Vec::new(),
    }
}
