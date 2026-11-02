//! Purpose:
//! Proves when temporary call roots cannot retire the last owner of a parameter payload.
//!
//! Called from:
//! - Caller-visible effect summaries and receiver-root canonicalization before inlining.
//!
//! Key details:
//! - Instruction effects remain conservative for intra-function ordering and validation.
//! - Only unchanged, concrete, by-value parameters keep the extra root non-destructing.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

/// Finds single-store hidden roots backed by a parameter that stays live through every call.
pub(crate) fn borrowed_parameter_roots(function: &Function) -> HashSet<LocalSlotId> {
    let mut unchanged = function.params.iter().enumerate().filter_map(|(index, parameter)| {
        let local = function.locals.get(index)?;
        (!parameter.by_ref && local.kind == LocalKind::PhpLocal
            && !matches!(local.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)))
            .then_some(local.id)
    }).collect::<HashSet<_>>();
    let mut stores = HashMap::new();
    let mut invalid = HashSet::new();
    for instruction in &function.instructions {
        match &instruction.immediate {
            Some(Immediate::LocalSlot(slot)) => {
                if instruction.op != Op::LoadLocal { unchanged.remove(slot); }
                if function.locals.get(slot.as_raw() as usize)
                    .is_none_or(|local| local.kind != LocalKind::HiddenTemp)
                {
                    continue;
                }
                match instruction.op {
                    Op::StoreLocal => {
                        if let Some(&value) = instruction.operands.first() {
                            if stores.insert(*slot, value).is_some() { invalid.insert(*slot); }
                        } else { invalid.insert(*slot); }
                    }
                    Op::PushCallOperandOwner | Op::PopCallOperandOwner | Op::ReleaseLocalSlot => {}
                    _ => { invalid.insert(*slot); }
                }
            }
            Some(Immediate::LocalSlotPair { first, second }) => {
                unchanged.remove(first);
                unchanged.remove(second);
                invalid.insert(*first);
                invalid.insert(*second);
            }
            Some(Immediate::IterStart(metadata)) => {
                for slot in metadata.local_slots() {
                    unchanged.remove(&slot);
                    invalid.insert(slot);
                }
            }
            _ => {}
        }
    }
    stores.into_iter().filter_map(|(slot, value)| {
        if invalid.contains(&slot) { return None; }
        let source = borrowed_parameter_source(function, value)?;
        unchanged.contains(&source).then_some(slot)
    }).collect()
}

/// Follows ownership-preserving SSA forwarding back to an unchanged parameter slot.
fn borrowed_parameter_source(function: &Function, mut value: ValueId) -> Option<LocalSlotId> {
    for _ in 0..8 {
        let ValueDef::Instruction { inst, .. } = function.value(value)?.def else { return None; };
        let instruction = function.instruction(inst)?;
        match instruction.op {
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(slot)) = &instruction.immediate else { return None; };
                return Some(*slot);
            }
            Op::Acquire | Op::Move | Op::Borrow => value = *instruction.operands.first()?,
            _ => return None,
        }
    }
    None
}
