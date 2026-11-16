//! Purpose:
//! Tests the inliner's proof for explicitly published, cleared temporary owners.
//!
//! Called from:
//! - The inline temporary analysis module's test harness.
//!
//! Key details:
//! - Checks both lowering orders and refuses missing clears or mismatched owner stacks.

use super::*;
use crate::ir::{Builder, IrHeapKind, IrType, Ownership, Terminator};
use crate::types::PhpType;

/// Builds one temporary lifetime with optional intervening work after unpublication.
fn fixture(store_first: bool, clear: bool, gap: bool) -> Function {
    let mut function = Function::new("scoped".to_string(), IrType::Void, PhpType::Void);
    let slot = function.add_local(None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, LocalKind::OwnedTemp);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let value = builder.emit(Op::ICheckedAdd, vec![one, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let mut ops = if store_first {
        vec![Op::StoreLocal, Op::PushCallOperandOwner]
    } else {
        vec![Op::PushCallOperandOwner, Op::StoreLocal]
    };
    ops.push(Op::PopCallOperandOwner);
    for op in ops {
        builder.emit(op, if op == Op::StoreLocal { vec![value] } else { vec![] },
            Some(Immediate::LocalSlot(slot)), IrType::Void, PhpType::Void, Ownership::NonHeap);
    }
    if gap { builder.emit_const_i64(2); }
    if clear {
        builder.emit(Op::UnsetLocal, vec![], Some(Immediate::LocalSlot(slot)),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
    }
    builder.terminate(Terminator::Return { value: None });
    function
}

/// Both generated publication orders are safe, but ownership may never survive the block.
#[test]
fn inline_scoped_temps_require_explicit_adjacent_retirement() {
    for store_first in [false, true] {
        assert_eq!(scoped_temps(&fixture(store_first, true, false)).len(), 1);
        assert!(scoped_temps(&fixture(store_first, false, false)).is_empty());
        assert!(scoped_temps(&fixture(store_first, true, true)).is_empty());
        let mut hidden = fixture(store_first, true, false);
        hidden.locals[0].kind = LocalKind::HiddenTemp;
        assert_eq!(scoped_temps(&hidden).len(), 1);
    }
}

/// The runtime pops the actual stack, so matching per-slot counts alone cannot justify a splice.
#[test]
fn inline_scoped_temps_require_matching_owner_stack() {
    let mut function = fixture(false, true, false);
    assert!(owner_scopes_are_balanced(&function));
    function.instructions.iter_mut().find(|inst| inst.op == Op::PopCallOperandOwner).unwrap()
        .immediate = Some(Immediate::LocalSlot(LocalSlotId::from_raw(1)));
    assert!(!owner_scopes_are_balanced(&function));
}

/// A slot spanning a branch cannot be justified by this deliberately block-local proof.
#[test]
fn inline_scoped_temps_refuse_cross_block_lifetimes() {
    let mut function = fixture(false, true, false);
    let clear = function.blocks[0].instructions.pop().unwrap();
    let entry = function.entry;
    let mut builder = Builder::new(&mut function);
    let exit = builder.create_named_block("exit", vec![]);
    builder.position_at_end(exit);
    builder.terminate(Terminator::Return { value: None });
    function.blocks[exit.as_raw() as usize].instructions.push(clear);
    function.blocks[entry.as_raw() as usize].terminator = Some(Terminator::Br { target: exit, args: vec![] });
    assert!(scoped_temps(&function).is_empty());
}
