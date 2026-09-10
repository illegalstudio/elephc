//! Purpose:
//! Identifies SSA owners whose cleanup belongs to a published call-operand slot.
//!
//! Called from:
//! - `super::FunctionContext` ownership-transfer and Mixed-boxing queries.
//!
//! Key details:
//! - Slot retirement is an ownership obligation even without an SSA Release instruction.
//! - Publishing a slot may consume its source; another consumer must retain a separate lease.
//! - Evaluation slots cleared with UnsetLocal transfer their owner and do not match this query.

use std::collections::HashSet;

use crate::ir::{Function, Immediate, InstId, Op, ValueId};

/// Returns whether a scoped caller slot will retire this SSA owner's reference.
pub(super) fn has_scoped_cleanup(
    function: &Function,
    value: ValueId,
    current: Option<InstId>,
) -> bool {
    let slots = function.instructions.iter().enumerate()
        .filter_map(|(index, inst)| {
            if inst.op != Op::StoreLocal || inst.operands != [value]
                || current == Some(InstId::from_raw(index as u32))
            {
                return None;
            }
            match inst.immediate {
                Some(Immediate::LocalSlot(slot)) => Some(slot),
                _ => None,
            }
        })
        .collect::<HashSet<_>>();
    if slots.is_empty() {
        return false;
    }
    let published = function.instructions.iter()
        .filter_map(|inst| {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
            (inst.op == Op::PushCallOperandOwner && slots.contains(&slot)).then_some(slot)
        })
        .collect::<HashSet<_>>();
    function.instructions.iter().any(|inst| {
        inst.op == Op::ReleaseLocalSlot && matches!(inst.immediate,
            Some(Immediate::LocalSlot(slot)) if published.contains(&slot))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, IrType, LocalKind, Ownership};
    use crate::types::PhpType;

    /// A caller's published slot blocks another consumer, but not its own publishing store.
    #[test]
    fn published_operand_owner_is_not_a_transferable_ssa_temporary() {
        for retire in [Op::ReleaseLocalSlot, Op::UnsetLocal] {
            let mut function = Function::new("scoped_owner".into(), IrType::Void, PhpType::Void);
            let (value, store, consumer) = {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let ty = PhpType::Callable;
                let ir_ty = IrType::from_php(&ty);
                let slot = builder.add_local(Some("operand".into()), ir_ty, ty.clone(), LocalKind::OwnedTemp);
                let value = builder.emit(Op::ClosureNew, Vec::new(), None, ir_ty, ty, Ownership::Owned).unwrap();
                let store = InstId::from_raw(builder.function().instructions.len() as u32);
                builder.emit(Op::StoreLocal, vec![value], Some(Immediate::LocalSlot(slot)), IrType::Void, PhpType::Void, Ownership::NonHeap);
                builder.emit(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)), IrType::Void, PhpType::Void, Ownership::NonHeap);
                let consumer = InstId::from_raw(builder.function().instructions.len() as u32);
                builder.emit(Op::MixedBox, vec![value], None, IrType::from_php(&PhpType::Mixed), PhpType::Mixed, Ownership::Owned);
                builder.emit(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)), IrType::Void, PhpType::Void, Ownership::NonHeap);
                builder.emit(retire, Vec::new(), Some(Immediate::LocalSlot(slot)), IrType::Void, PhpType::Void, Ownership::NonHeap);
                (value, store, consumer)
            };
            assert!(!has_scoped_cleanup(&function, value, Some(store)), "the root store may adopt its source");
            assert_eq!(has_scoped_cleanup(&function, value, Some(consumer)), retire == Op::ReleaseLocalSlot);
        }
    }
}
