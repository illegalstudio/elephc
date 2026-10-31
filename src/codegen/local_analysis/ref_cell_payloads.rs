//! Purpose:
//! Identifies stable physical payload types for local reference-cell pointers.
//!
//! Called from:
//! - LocalSlotAnalysis construction before target assembly emission.
//!
//! Key details:
//! - The binding instruction records the payload type before eval or control flow widens the slot.
//! - Conflicting bindings fall back to the final local type instead of guessing one ABI layout.
//! - Object class refinements and container element refinements share one physical payload shape.

use super::*;

/// Finds exclusively local Mixed promotions without inferring layout from a widened alias slot.
pub(super) fn owned_mixed_slots(function: &Function) -> HashSet<LocalSlotId> {
    let mut candidates = HashSet::new();
    let mut external = initially_ref_cell_slots(function);
    for inst in &function.instructions {
        let Some(slot) =
            ref_cell_target_slot(function, inst.op, inst.immediate.as_ref(), &inst.operands)
        else {
            continue;
        };
        match (inst.op, inst.immediate.as_ref()) {
            (Op::PromoteLocalRefCell, Some(Immediate::LocalSlotPair { second, .. })) => {
                if function.locals.get(second.as_raw() as usize).is_some_and(|owner| {
                    owner.kind == crate::ir::LocalKind::RefCell
                        && owner.php_type.codegen_repr() == PhpType::Mixed
                }) {
                    candidates.insert(slot);
                } else {
                    external.insert(slot);
                }
            }
            // Capturing an existing local cell adds an owner without changing its payload ABI.
            (Op::ClosureCapture, _) => {}
            _ => {
                external.insert(slot);
            }
        }
    }
    candidates.retain(|slot| !external.contains(slot));
    candidates
}

/// Finds slots whose every possible reference binding uses one compatible physical payload type.
pub(super) fn stable_payload_types(function: &Function) -> HashMap<LocalSlotId, PhpType> {
    let mut candidates = HashMap::new();
    let mut conflicts = HashSet::new();
    for (index, param) in function.params.iter().enumerate().filter(|(_, param)| param.by_ref) {
        let slot = LocalSlotId::from_raw(index as u32);
        if function.locals.get(index).is_some() {
            record_payload_type(&mut candidates, &mut conflicts, slot, param.php_type.clone());
        }
    }
    for inst in &function.instructions {
        let Some(slot) =
            ref_cell_target_slot(function, inst.op, inst.immediate.as_ref(), &inst.operands)
        else {
            continue;
        };
        let payload_ty = match (inst.op, inst.immediate.as_ref()) {
            (Op::PromoteLocalRefCell, Some(Immediate::LocalSlotPair { second, .. })) => {
                function
                    .locals
                    .get(second.as_raw() as usize)
                    .map(|owner| owner.php_type.clone())
            }
            (Op::AliasLocalRefCell, Some(Immediate::LocalSlotPair { second, .. })) => {
                if conflicts.contains(second) {
                    conflicts.insert(slot);
                    candidates.remove(&slot);
                    None
                } else {
                    candidates
                        .get(second)
                        .cloned()
                        .or_else(|| Some(inst.result_php_type.clone()))
                }
            }
            (Op::BindRefCellPtr | Op::AdoptRefCellPtr, _) => {
                Some(inst.result_php_type.clone())
            }
            // Capturing an existing cell adds an owner without changing its payload ABI. Iterator
            // bindings expose external element storage, but their void instruction metadata does
            // not describe that storage's payload type, so they retain the conservative fallback.
            (Op::ClosureCapture | Op::IterCurrentValueRef, _) => None,
            _ => None,
        };
        if let Some(payload_ty) = payload_ty {
            record_payload_type(&mut candidates, &mut conflicts, slot, payload_ty);
        }
    }
    for slot in conflicts {
        candidates.remove(&slot);
    }
    candidates
}

/// Records one binding while rejecting slots whose bindings need different load or boxing rules.
fn record_payload_type(
    candidates: &mut HashMap<LocalSlotId, PhpType>,
    conflicts: &mut HashSet<LocalSlotId>,
    slot: LocalSlotId,
    ty: PhpType,
) {
    if conflicts.contains(&slot) {
        return;
    }
    let ty = ty.codegen_repr();
    let Some(existing) = candidates.get(&slot) else {
        candidates.insert(slot, ty);
        return;
    };
    if payload_types_are_compatible(existing, &ty) {
        return;
    }
    candidates.remove(&slot);
    conflicts.insert(slot);
}

/// Treats logical refinements as one payload only when loading and boxing remain identical.
fn payload_types_are_compatible(left: &PhpType, right: &PhpType) -> bool {
    left == right
        || matches!(
            (left, right),
            (PhpType::Object(_), PhpType::Object(_))
                | (PhpType::Array(_), PhpType::Array(_))
                | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
        )
}
