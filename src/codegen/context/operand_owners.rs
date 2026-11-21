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

use crate::ir::{BlockId, Function, Immediate, InstId, Op, ValueId};

/// Finds releases reachable after a consumer, retaining all implicit handlers conservatively.
/// A sibling null/fatal branch does not own cleanup on the non-null call path. Loop
/// re-execution of the value's producer begins a new dynamic owner, not the lease
/// this consumer can take. Missing instruction/block metadata keeps the old conservative answer.
pub(super) fn has_reachable_release(
    function: &Function, value: ValueId, current: Option<InstId>,
) -> bool {
    let releases_value = |inst: &crate::ir::Instruction| {
        inst.op == Op::Release && inst.operands.first().copied() == Some(value)
    };
    if !function.instructions.iter().any(releases_value) { return false; }
    let Some((block, position)) = current.and_then(|current| {
        function.blocks.iter().find_map(|block| {
            block.instructions.iter().position(|inst| *inst == current)
                .map(|position| (block.id, position + 1))
        })
    }) else {
        return function.instructions.iter().any(releases_value);
    };
    let mut pending = vec![(block, position)];
    // Handler edges are implicit in EIR. Include every handler rather than
    // treating the normal terminator graph as proof that catch cleanup is absent.
    pending.extend(function.instructions.iter().filter_map(|inst| {
        if inst.op != Op::TryPushHandler { return None; }
        let Some(Immediate::I64(token)) = inst.immediate else { return None; };
        Some((BlockId::from_raw(token as u32), 0))
    }));
    let mut visited = HashSet::new();
    while let Some((block_id, start)) = pending.pop() {
        if !visited.insert((block_id, start)) { continue; }
        let Some(block) = function.blocks.get(block_id.as_raw() as usize) else { return true; };
        if start == 0 && block.params.contains(&value) { continue; }
        let mut redefined = false;
        for instruction in block.instructions.iter().skip(start) {
            let Some(inst) = function.instruction(*instruction) else { return true; };
            if inst.result == Some(value) { redefined = true; break; }
            if releases_value(inst) { return true; }
        }
        if redefined { continue; }
        let Some(term) = &block.terminator else { return true; };
        pending.extend(crate::codegen::local_analysis::terminator_successors(term)
            .into_iter().map(|block| (block, 0)));
    }
    false
}

/// Returns whether a scoped caller slot will retire this SSA owner's reference.
pub(super) fn has_scoped_cleanup(
    function: &Function,
    value: ValueId,
    current: Option<InstId>,
) -> bool {
    let stores = function.instructions.iter().enumerate()
        .filter_map(|(index, inst)| {
            if inst.op != Op::StoreLocal || inst.operands != [value]
                || current == Some(InstId::from_raw(index as u32))
            {
                return None;
            }
            match inst.immediate {
                Some(Immediate::LocalSlot(slot)) => Some((index, slot)),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    if stores.is_empty() {
        return false;
    }
    let slots = stores.iter().map(|(_, slot)| *slot).collect::<HashSet<_>>();
    let published = function.instructions.iter()
        .filter_map(|inst| {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
            (inst.op == Op::PushCallOperandOwner && slots.contains(&slot)).then_some(slot)
        })
        .collect::<HashSet<_>>();
    function.instructions.iter().enumerate().any(|(release_index, inst)| {
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return false; };
        inst.op == Op::ReleaseLocalSlot
            && published.contains(&slot)
            && stores.iter().any(|(store_index, store_slot)| {
                *store_slot == slot && *store_index < release_index
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, IrType, LocalKind, Ownership, Terminator};
    use crate::types::PhpType;

    /// Builds sibling fatal/call paths with optional normal, catch or loop cleanup.
    fn release_paths(
        normal_cleanup: bool, catch_cleanup: bool, loop_back: bool,
    ) -> (Function, ValueId, InstId) {
        let mut function = Function::new("release_paths".into(), IrType::Void, PhpType::Void);
        let (value, consumer) = {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let fatal = builder.create_named_block("null.fatal", Vec::new());
            let call = builder.create_named_block("non_null.call", Vec::new());
            let handler = builder.create_named_block("catch", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let value = builder.emit(Op::ObjectNew, Vec::new(), None,
                IrType::from_php(&PhpType::Object("Fiber".into())),
                PhpType::Object("Fiber".into()), Ownership::Owned).unwrap();
            if catch_cleanup {
                builder.emit(Op::TryPushHandler, Vec::new(), Some(Immediate::I64(handler.as_raw() as i64)),
                    IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let condition = builder.emit_const_bool(true);
            builder.terminate(Terminator::CondBr {
                cond: condition, then_target: call, then_args: Vec::new(),
                else_target: fatal, else_args: Vec::new(),
            });
            builder.position_at_end(fatal);
            builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            builder.terminate(Terminator::Unreachable);
            builder.position_at_end(call);
            let consumer = InstId::from_raw(builder.function().instructions.len() as u32);
            builder.emit(Op::Call, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            if normal_cleanup {
                builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            builder.terminate(if loop_back {
                Terminator::Br { target: entry, args: Vec::new() }
            } else { Terminator::Return { value: None } });
            builder.position_at_end(handler);
            builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            builder.terminate(Terminator::Return { value: None });
            (value, consumer)
        };
        (function, value, consumer)
    }

    /// A sibling null/fatal release cannot suppress transfer on the non-null consumer path.
    #[test]
    fn unreachable_sibling_release_does_not_block_owner_transfer() {
        let (function, value, consumer) = release_paths(false, false, false);
        assert!(!has_reachable_release(&function, value, Some(consumer)));
        assert!(has_reachable_release(&function, value, None));
        assert!(has_reachable_release(&function, value, Some(InstId::from_raw(u32::MAX))));
    }

    /// Normal-path and implicit handler cleanup still retain the consumer's input lease.
    #[test]
    fn reachable_and_exceptional_releases_block_owner_transfer() {
        for (normal, caught) in [(true, false), (false, true), (true, true)] {
            let (function, value, consumer) = release_paths(normal, caught, false);
            assert!(has_reachable_release(&function, value, Some(consumer)));
        }
    }

    /// A loop re-executing the producer releases a new dynamic owner, not the consumed lease.
    #[test]
    fn producer_reexecution_ends_the_previous_dynamic_owner_path() {
        let (function, value, consumer) = release_paths(false, false, true);
        assert!(!has_reachable_release(&function, value, Some(consumer)));
        let (function, value, consumer) = release_paths(true, false, true);
        assert!(has_reachable_release(&function, value, Some(consumer)));
    }

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
