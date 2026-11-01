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

#[cfg(test)]
use crate::ir::{Function, InstId, ValueId};

/// Returns whether a scoped caller slot will retire this SSA owner's reference.
#[cfg(test)]
pub(super) fn has_scoped_cleanup(
    function: &Function,
    value: ValueId,
    current: Option<InstId>,
) -> bool {
    // Codegen asks through the body's cached `ValueUseIndex`; this entry builds one for a single
    // question, which is what the unit tests below exercise.
    super::value_uses::ValueUseIndex::build(function)
        .has_scoped_cleanup(value, current.map(|inst| inst.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, Immediate, IrType, LocalKind, Op, Ownership};
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

    /// A loop's stale-slot cleanup runs before publishing the current call result and therefore
    /// cannot own that new SSA value. The following `UnsetLocal` transfers the staged result back
    /// to the expression, so a later container store must be allowed to adopt it without retaining
    /// an extra reference.
    #[test]
    fn prepublished_loop_result_ignores_cleanup_before_its_store() {
        let mut function = Function::new("loop_call_result".into(), IrType::Void, PhpType::Void);
        let (value, consumer) = {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let ty = PhpType::Object("Item".into());
            let ir_ty = IrType::from_php(&ty);
            let slot = builder.add_local(
                Some("staged_result".into()),
                ir_ty,
                ty.clone(),
                LocalKind::OwnedTemp,
            );
            builder.emit(
                Op::PushCallOperandOwner,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let value = builder
                .emit(Op::ObjectNew, Vec::new(), None, ir_ty, ty, Ownership::Owned)
                .unwrap();
            builder.emit(
                Op::ReleaseLocalSlot,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::StoreLocal,
                vec![value],
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::PopCallOperandOwner,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::UnsetLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let consumer = InstId::from_raw(builder.function().instructions.len() as u32);
            builder.emit(
                Op::HashSet,
                vec![value],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            (value, consumer)
        };
        assert!(!has_scoped_cleanup(&function, value, Some(consumer)));
    }
}
