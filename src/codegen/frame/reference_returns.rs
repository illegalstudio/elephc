//! Purpose:
//! Identifies the reference-cell lease transferred by an exact return snapshot.
//!
//! Called from:
//! - By-reference return terminator lowering before the normal frame epilogue.
//!
//! Key details:
//! - Other pending return sites retain independent leases until cancelled or unwound.
//! - Only the lease belonging to the selected snapshot escapes normal cleanup.

use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op, ValueDef, ValueId};

/// Follows representation-preserving copies to the acquisition that owns a returned snapshot.
pub(in crate::codegen) fn owner_slot(function: &Function, mut value: ValueId) -> Option<LocalSlotId> {
    for _ in 0..function.values.len() {
        let ValueDef::Instruction { inst, .. } = function.value(value)?.def else { return None; };
        let instruction = function.instruction(inst)?;
        if instruction.op == Op::AcquireRefCell {
            let Some(Immediate::LocalSlot(owner)) = instruction.immediate else { return None; };
            return function.locals.get(owner.as_raw() as usize)
                .filter(|local| local.kind == LocalKind::ReturnRefCell).map(|_| owner);
        }
        if !matches!(instruction.op, Op::Move | Op::Borrow | Op::Acquire) { return None; }
        value = *instruction.operands.first()?;
    }
    None
}
