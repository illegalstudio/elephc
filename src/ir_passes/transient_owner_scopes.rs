//! Purpose:
//! Recognizes complete block-local owner scopes used to protect one temporary value.
//!
//! Called from:
//! - Numeric and loop representation passes that may erase a proven scalar temporary.
//!
//! Key details:
//! - A scope is accepted only when one private slot has exactly one store, push, pop and release.
//! - The same slot may host independent complete scopes in mutually exclusive blocks.
//! - Cross-block lifetimes, incomplete scopes and otherwise-addressed slots fail closed.

use std::collections::HashSet;

use crate::ir::{
    BlockId, Function, Immediate, InstId, LocalKind, LocalSlotId, Op, ValueDef, ValueId,
};

/// One complete temporary owner publication and retirement in a single basic block.
#[derive(Clone)]
pub(super) struct TransientOwnerScope {
    pub(super) block: BlockId,
    pub(super) slot: LocalSlotId,
    pub(super) store: InstId,
    operations: Vec<InstId>,
}

impl TransientOwnerScope {
    /// Iterates over the store, publication, detachment and release instructions.
    pub(super) fn operations(&self) -> impl Iterator<Item = InstId> + '_ {
        self.operations.iter().copied()
    }

    /// Returns the transparent owner-copy chain from this scope's stored value to `source`.
    pub(super) fn forwarding_chain_to(
        &self,
        function: &Function,
        source: ValueId,
    ) -> Option<Vec<InstId>> {
        let mut value = *function.instruction(self.store)?.operands.first()?;
        let mut chain = Vec::new();
        for _ in 0..8 {
            if value == source {
                return Some(chain);
            }
            let ValueDef::Instruction { block, inst, .. } = function.value(value)?.def else {
                return None;
            };
            let producer = function.instruction(inst)?;
            if block != self.block
                || !matches!(
                    producer.op,
                    Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned | Op::MixedClone
                )
                || producer.immediate.is_some()
                || producer.operands.len() != 1
            {
                return None;
            }
            chain.push(inst);
            value = producer.operands[0];
        }
        None
    }

    /// Returns the forwarding chain only when none of its temporary results escape this scope.
    pub(super) fn exclusive_forwarding_chain_to(
        &self,
        function: &Function,
        source: ValueId,
    ) -> Option<Vec<InstId>> {
        let chain = self.forwarding_chain_to(function, source)?;
        let mut allowed = self.operations.iter().copied().collect::<HashSet<_>>();
        allowed.extend(chain.iter().copied());
        for id in &chain {
            let result = function.instruction(*id)?.result?;
            if function.instructions.iter().enumerate().any(|(raw, instruction)| {
                instruction.operands.contains(&result)
                    && !allowed.contains(&InstId::from_raw(raw as u32))
            }) || function
                .blocks
                .iter()
                .filter_map(|block| block.terminator.as_ref())
                .any(|terminator| terminator.used_values().contains(&result))
            {
                return None;
            }
        }
        Some(chain)
    }
}

/// Collects every complete private owner scope in the function.
pub(super) fn collect(function: &Function) -> Vec<TransientOwnerScope> {
    function
        .locals
        .iter()
        .filter(|local| matches!(local.kind, LocalKind::HiddenTemp | LocalKind::OwnedTemp))
        .flat_map(|local| {
            function
                .blocks
                .iter()
                .filter_map(move |block| scope_for_block_slot(function, block.id, local.id))
        })
        .collect()
}

/// Returns the complete private owner scope whose publication store is `store`.
pub(super) fn for_store(function: &Function, store: InstId) -> Option<TransientOwnerScope> {
    let instruction = function.instruction(store)?;
    let Some(Immediate::LocalSlot(slot)) = instruction.immediate else {
        return None;
    };
    let block = function
        .blocks
        .iter()
        .find(|block| block.instructions.contains(&store))?
        .id;
    let scope = scope_for_block_slot(function, block, slot)?;
    (scope.store == store).then_some(scope)
}

/// Validates the complete operation inventory of one private owner slot.
fn scope_for_block_slot(
    function: &Function,
    block: BlockId,
    slot: LocalSlotId,
) -> Option<TransientOwnerScope> {
    let local = function.locals.get(slot.as_raw() as usize)?;
    if !matches!(local.kind, LocalKind::HiddenTemp | LocalKind::OwnedTemp) {
        return None;
    }
    let mut operations = Vec::new();
    for &id in &function.block(block)?.instructions {
        let inst = function.instruction(id)?;
        if inst.immediate == Some(Immediate::LocalSlot(slot)) {
            operations.push((id, inst.op));
        }
    }
    let ops = operations.iter().map(|(_, op)| *op).collect::<Vec<_>>();
    if !matches!(
        ops.as_slice(),
        [
            Op::StoreLocal,
            Op::PushCallOperandOwner,
            Op::PopCallOperandOwner,
            Op::ReleaseLocalSlot
        ] | [
            Op::PushCallOperandOwner,
            Op::StoreLocal,
            Op::PopCallOperandOwner,
            Op::ReleaseLocalSlot
        ]
    ) {
        return None;
    }
    let store = operations.iter().find_map(|(id, op)| (*op == Op::StoreLocal).then_some(*id))?;
    Some(TransientOwnerScope {
        block,
        slot,
        store,
        operations: operations.into_iter().map(|(id, _)| id).collect(),
    })
}
