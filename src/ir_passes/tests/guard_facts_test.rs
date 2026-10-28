//! Purpose:
//! Verifies repeated SSA guards without carrying a branch outcome across a refreshed loop value.
//!
//! Called from:
//! - The EIR pass test harness.
//!
//! Key details:
//! - Entry re-execution is an explicit regression for backward-edge dominance.

use crate::ir::{Builder, Function, IrType, LocalKind, Terminator};
use crate::types::PhpType;

/// A dominating one-way guard establishes its condition for a later test of the same SSA value.
#[test]
fn checked_managed_buffer_dominating_guard_fact() {
    let mut function = Function::new("guard".into(), IrType::Void, PhpType::Void);
    let (later, success);
    {
        let mut builder = Builder::new(&mut function);
        let slot = builder.add_local(Some("flag".into()), IrType::I64, PhpType::Bool, LocalKind::PhpLocal);
        let entry = builder.create_named_block("entry", vec![]);
        later = builder.create_named_block("later", vec![]);
        success = builder.create_named_block("success", vec![]);
        let failure = builder.create_named_block("failure", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let cond = builder.emit_load_local(slot, IrType::I64, PhpType::Bool);
        builder.terminate(Terminator::CondBr { cond, then_target: later, then_args: vec![], else_target: failure, else_args: vec![] });
        builder.position_at_end(later);
        builder.terminate(Terminator::CondBr { cond, then_target: success, then_args: vec![], else_target: failure, else_args: vec![] });
        builder.position_at_end(success);
        builder.terminate(Terminator::Return { value: None });
        builder.position_at_end(failure);
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(super::super::guard_facts::fold(&mut function));
    assert_eq!(function.blocks[later.as_raw() as usize].terminator,
        Some(Terminator::Br { target: success, args: vec![] }));
    crate::ir::validate_function(&function).unwrap();
}

/// Re-entering the function entry cannot prove a freshly loaded condition's next outcome.
#[test]
fn checked_managed_buffer_guard_fact_does_not_cross_entry_reexecution() {
    let mut function = Function::new("reentry".into(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let slot = builder.add_local(Some("flag".into()), IrType::I64, PhpType::Bool, LocalKind::PhpLocal);
        let entry = builder.create_named_block("entry", vec![]);
        let guard = builder.create_named_block("guard", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let cond = builder.emit_load_local(slot, IrType::I64, PhpType::Bool);
        builder.terminate(Terminator::CondBr { cond, then_target: guard, then_args: vec![], else_target: exit, else_args: vec![] });
        builder.position_at_end(guard);
        builder.terminate(Terminator::CondBr { cond, then_target: entry, then_args: vec![], else_target: exit, else_args: vec![] });
        builder.position_at_end(exit);
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(super::super::guard_facts::fold(&mut function));
    assert!(matches!(function.blocks[function.entry.as_raw() as usize].terminator, Some(Terminator::CondBr { .. })));
    crate::ir::validate_function(&function).unwrap();
}
