//! Purpose:
//! Identifies reference cells whose boxed payload layout is owned by the current frame.
//!
//! Called from:
//! - LocalSlotAnalysis construction before target assembly emission.
//!
//! Key details:
//! - Incoming references and rebound aliases keep their declared view of external storage.
//! - A local promotion's owner slot records the actual payload cleanup contract.

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
                })
                {
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
