//! Purpose:
//! Verifies that reading a receiver never exposes the address of its local slot.
//!
//! Called from:
//! - The EIR pass library test harness.
//!
//! Key details:
//! - Method arguments keep the conservative alias rule, even when they reuse the receiver.

use crate::ir::{
    validate_function, Builder, DataId, DataPool, Function, Immediate, IrType, LocalKind, LocalSlotId, Op, Ownership,
    Terminator,
};
use crate::ir_passes::by_ref_alias::address_escaping_slots;
use crate::ir_passes::dead_store::DeadStore;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Builds a receiver consumer and optionally passes its loaded receiver as an argument too.
fn receiver_consumer(op: Op, php_type: PhpType, repeat_receiver: bool) -> (Function, LocalSlotId, LocalSlotId) {
    let mut function = Function::new("receiver_consumer".into(), IrType::Void, PhpType::Void);
    let ir_type = IrType::from_php(&php_type);
    let receiver_slot = function.add_local(
        Some("receiver".into()), ir_type, php_type.clone(), LocalKind::PhpLocal,
    );
    let argument_slot = function.add_local(
        Some("argument".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal,
    );
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let receiver = builder.emit_load_local(receiver_slot, ir_type, php_type);
        let mut operands = vec![receiver];
        if op == Op::MethodCall {
            let argument = if repeat_receiver {
                receiver
            } else {
                builder.emit_load_local(argument_slot, IrType::I64, PhpType::Int)
            };
            operands.push(argument);
        }
        builder.emit(
            op, operands, Some(Immediate::Data(DataId::from_raw(0))),
            IrType::I64, PhpType::Int, Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    (function, receiver_slot, argument_slot)
}

/// Concrete, boxed and extern pointer receivers do not expose their local slot addresses.
#[test]
fn property_receiver_load_is_value_only() {
    for php_type in [
        PhpType::Object("Receiver".into()), PhpType::Mixed,
        PhpType::Pointer(Some("ExternReceiver".into())),
    ] {
        let (function, receiver, _) = receiver_consumer(Op::PropGet, php_type, false);
        assert!(!address_escaping_slots(&function).contains(&receiver));
    }
}

/// A pointer receiver keeps its observed store live while an overwritten prior store dies.
#[test]
fn pointer_property_receiver_allows_dead_store_elimination() {
    let mut function = Function::new("pointer_receiver".into(), IrType::I64, PhpType::Int);
    let pointer_type = PhpType::Pointer(Some("ExternReceiver".into()));
    let slot = function.add_local(
        Some("receiver".into()), IrType::I64, pointer_type.clone(), LocalKind::PhpLocal,
    );
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        // Synthetic pointer constants are only inspected as EIR, never dereferenced.
        let earlier = builder.emit(
            Op::ConstI64, vec![], Some(Immediate::I64(32)),
            IrType::I64, pointer_type.clone(), Ownership::NonHeap,
        ).unwrap();
        builder.emit_store_local(slot, earlier);
        let later = builder.emit(
            Op::ConstI64, vec![], Some(Immediate::I64(64)),
            IrType::I64, pointer_type.clone(), Ownership::NonHeap,
        ).unwrap();
        builder.emit_store_local(slot, later);
        let receiver = builder.emit_load_local(slot, IrType::I64, pointer_type);
        let field = builder.emit(
            Op::PropGet, vec![receiver], Some(Immediate::Data(DataId::from_raw(0))),
            IrType::I64, PhpType::Int, Ownership::NonHeap,
        ).unwrap();
        builder.terminate(Terminator::Return { value: Some(field) });
    }
    validate_function(&function).expect("pointer receiver EIR is valid");
    assert!(DeadStore.run(&mut function, &mut DataPool::default()));
    assert_eq!(function.instructions[1].op, Op::Nop);
    assert_eq!(function.instructions[3].op, Op::StoreLocal);
    validate_function(&function).expect("dead store elimination preserves valid EIR");
}

/// Dispatch reads only the receiver by value while keeping ordinary arguments conservative.
#[test]
fn method_receiver_load_is_value_only_but_arguments_escape() {
    for php_type in [PhpType::Object("Receiver".into()), PhpType::Mixed] {
        let (function, receiver, argument) = receiver_consumer(Op::MethodCall, php_type, false);
        let escaping = address_escaping_slots(&function);
        assert!(!escaping.contains(&receiver));
        assert!(escaping.contains(&argument));
    }
}

/// Using the identical value as receiver and by-reference-capable argument still escapes.
#[test]
fn repeated_method_receiver_argument_still_escapes() {
    let (function, receiver, _) = receiver_consumer(
        Op::MethodCall, PhpType::Object("Receiver".into()), true,
    );
    assert!(address_escaping_slots(&function).contains(&receiver));
}

/// Unclassified property writes retain their existing conservative receiver classification.
#[test]
fn property_write_receiver_remains_conservative() {
    let (function, receiver, _) = receiver_consumer(
        Op::PropGetForWrite, PhpType::Object("Receiver".into()), false,
    );
    assert!(address_escaping_slots(&function).contains(&receiver));
}
