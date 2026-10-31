//! Purpose:
//! Regression tests for the direct-borrow `array_sum_int_value` source rewrite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers the accepted local-source scaffold and the conservative
//!   rejection when the source slot is address-escaping or written before the call.

use crate::ir::{
    Builder, DataPool, Function, Immediate, IrHeapKind, IrType, LocalKind, LocalSlotId, Op,
    Ownership, RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::ir_passes::array_sum_int_direct::ArraySumIntDirect;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Runs the direct-source pass once with an unused literal pool.
fn direct(function: &mut Function) -> bool {
    ArraySumIntDirect.run(function, &mut DataPool::default())
}

/// Builds the raw reducer call with a stable local source and its temporary-owner scaffold.
fn build_fixture(function: &mut Function, escape_source: bool, write_source: bool) -> (ValueId, LocalSlotId, LocalSlotId) {
    let array_type = IrType::Heap(IrHeapKind::Array);
    let array_php = PhpType::Array(Box::new(PhpType::Int));
    let source = function.add_local(
        Some("source".to_string()),
        array_type,
        array_php.clone(),
        LocalKind::PhpLocal,
    );
    let temp = function.add_local(
        Some("temp".to_string()),
        array_type,
        array_php.clone(),
        LocalKind::HiddenTemp,
    );
    let mut builder = Builder::new(function);
    let entry = builder.create_named_block("entry", vec![(array_type, array_php.clone())]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let parameter = builder.function().block(entry).unwrap().params[0];
    let _ = builder.emit(
        Op::StoreLocal,
        vec![parameter],
        Some(Immediate::LocalSlot(source)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let loaded = builder.emit_load_local(source, array_type, array_php.clone());
    if escape_source {
        let _ = builder.emit(
            Op::RuntimeCall,
            vec![loaded],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArraySum,
            ))),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        );
    }
    if write_source {
        let _ = builder.emit(
            Op::StoreLocal,
            vec![parameter],
            Some(Immediate::LocalSlot(source)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }
    let acquired = builder
        .emit(
            Op::Acquire,
            vec![loaded],
            None,
            array_type,
            array_php.clone(),
            Ownership::Owned,
        )
        .expect("acquired source");
    let _ = builder.emit(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(temp)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder.emit(
        Op::StoreLocal,
        vec![acquired],
        Some(Immediate::LocalSlot(temp)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder.emit(
        Op::PushCallOperandOwner,
        Vec::new(),
        Some(Immediate::LocalSlot(temp)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![acquired],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArraySumInt,
            ))),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
            RuntimeFnId::ArraySumInt.effects(),
            None,
        )
        .expect("raw sum result");
    let _ = builder.emit(
        Op::PopCallOperandOwner,
        Vec::new(),
        Some(Immediate::LocalSlot(temp)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder.emit(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(temp)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    (loaded, source, temp)
}

/// The stable local source becomes the reducer operand and the temporary scaffold is retired.
#[test]
fn rewrites_stable_local_source_to_a_direct_borrow() {
    let mut function = Function::new("direct_sum".to_string(), IrType::I64, PhpType::Int);
    let (loaded, source, temp) = build_fixture(&mut function, false, false);
    assert!(direct(&mut function));
    let call = function
        .instructions
        .iter()
        .find(|inst| {
            inst.immediate
                == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArraySumInt,
                )))
        })
        .expect("raw reducer call");
    assert_eq!(call.operands.as_slice(), [loaded]);
    assert!(!function.instructions.iter().any(|inst| {
        matches!(
            inst.op,
            Op::Acquire
                | Op::PushCallOperandOwner
                | Op::PopCallOperandOwner
                | Op::ReleaseLocalSlot
        ) || (inst.op == Op::StoreLocal
            && inst.immediate == Some(Immediate::LocalSlot(temp)))
    }));
    assert_eq!(
        function.locals[temp.as_raw() as usize].kind,
        LocalKind::BorrowedTemp
    );
    assert_eq!(
        function.locals[source.as_raw() as usize].kind,
        LocalKind::PhpLocal
    );
}

/// A source slot that may be written through a reference keeps the owner scaffold.
#[test]
fn rejects_an_address_escaping_source() {
    let mut function = Function::new("escaped_sum".to_string(), IrType::I64, PhpType::Int);
    build_fixture(&mut function, true, false);
    assert!(!direct(&mut function));
    assert!(function
        .instructions
        .iter()
        .any(|inst| inst.op == Op::PushCallOperandOwner));
}

/// A source slot written between the load and the call keeps the owner scaffold.
#[test]
fn rejects_a_source_write_before_the_call() {
    let mut function = Function::new("rewritten_sum".to_string(), IrType::I64, PhpType::Int);
    build_fixture(&mut function, false, true);
    assert!(!direct(&mut function));
    assert!(function
        .instructions
        .iter()
        .any(|inst| inst.op == Op::PushCallOperandOwner));
}
