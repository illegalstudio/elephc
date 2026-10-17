//! Purpose:
//! Recognizes destructor-free numeric argument owners that can retire before a tail branch.
//!
//! Called from:
//! - Tail frame eligibility and tail-site discovery.
//!
//! Key details:
//! - Checked arithmetic produces boxed int/float values even for typed int parameters.
//! - Each accepted root has one store/push/pop/clear sequence, balanced on every normal exit.
//! - Arbitrary Mixed values remain excluded because releasing them can run user code.

use std::collections::HashSet;
use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op, ValueId};
use crate::types::PhpType;
use super::super::rewrite::defining_instruction;

/// Proves that a value is scalar or a fresh box whose payload can only be numeric.
pub(super) fn is_numeric(function: &Function, mut value: ValueId) -> bool {
    for _ in 0..function.values.len() {
        let Some(metadata) = function.value(value) else { return false; };
        if matches!(metadata.php_type, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False) {
            return true;
        }
        let Some(inst) = defining_instruction(function, value) else { return false; };
        match inst.op {
            Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul | Op::ICheckedPow => return true,
            Op::Acquire | Op::Borrow | Op::Move | Op::MixedBox => {
                let Some(&operand) = inst.operands.first() else { return false; };
                value = operand;
            }
            _ => return false,
        }
    }
    false
}

/// Identifies temporary numeric roots fully retired on every normal return path.
pub(super) fn balanced_roots(function: &Function) -> HashSet<LocalSlotId> {
    function.locals.iter().filter(|local| {
        local.php_type == PhpType::Mixed
            && matches!(local.kind, LocalKind::HiddenTemp | LocalKind::OwnedTemp)
    }).filter(|local| {
        let mut uses = Vec::new();
        for inst in &function.instructions {
            match inst.immediate {
                Some(Immediate::LocalSlot(slot)) if slot == local.id => uses.push(inst),
                Some(Immediate::LocalSlotPair { first, second }) if first == local.id || second == local.id => return false,
                Some(Immediate::IterStart(metadata)) if metadata.local_slots().any(|slot| slot == local.id) => return false,
                _ => {}
            }
        }
        uses.len() == 4
            && [Op::StoreLocal, Op::PushCallOperandOwner, Op::PopCallOperandOwner].iter()
                .all(|&op| uses.iter().filter(|inst| inst.op == op).count() == 1)
            && uses.iter().filter(|inst| matches!(inst.op, Op::ReleaseLocalSlot | Op::UnsetLocal)).count() == 1
            && uses.iter().find(|inst| inst.op == Op::StoreLocal).and_then(|inst| inst.operands.first())
                .is_some_and(|&value| is_numeric(function, value))
            && normal_exits_are_clear(function, local.id)
    }).map(|local| local.id).collect()
}

/// Proves owner-slot and unwind-record balance over every explicit normal CFG path.
/// Four possible states per block bound the work even when the body already contains loops.
fn normal_exits_are_clear(function: &Function, slot: LocalSlotId) -> bool {
    let mut seen = vec![0_u8; function.blocks.len()];
    let mut work = vec![(function.entry, 0_u8)];
    while let Some((block, mut state)) = work.pop() {
        if seen[block.as_raw() as usize] & (1 << state) != 0 { continue; }
        seen[block.as_raw() as usize] |= 1 << state;
        let block = &function.blocks[block.as_raw() as usize];
        for &id in &block.instructions {
            let inst = &function.instructions[id.as_raw() as usize];
            if inst.immediate != Some(Immediate::LocalSlot(slot)) { continue; }
            state = match (inst.op, state) {
                (Op::StoreLocal, 0) => 1,
                (Op::PushCallOperandOwner, 1) => 3,
                (Op::PopCallOperandOwner, 3) => 1,
                (Op::ReleaseLocalSlot | Op::UnsetLocal, 1) => 0,
                _ => return false,
            };
        }
        if let Some(term) = &block.terminator {
            if matches!(term, crate::ir::Terminator::Return { .. }) && state != 0 { return false; }
            work.extend(super::super::cfg::successors(term).into_iter().map(|next| (next, state)));
        }
    }
    true
}
