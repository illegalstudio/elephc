//! Purpose:
//! Regression tests for the count-only `count(json_decode($s, true))` sink.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers the accepted decode/count/slot graph and a source released
//!   after the call, which is safe because the count is folded into the decode call.

use crate::ir::{
    Builder, DataPool, Function, Immediate, IrHeapKind, IrType, LocalSlotId, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::json_decode_count_sink::JsonDecodeCountSink;
use crate::types::PhpType;

/// Runs the sink once with an unused literal pool.
fn sink(function: &mut Function) -> bool {
    JsonDecodeCountSink.run(function, &mut DataPool::default())
}

/// Builds the accepted `count(json_decode($s, true))` graph into slot 0.
fn build_fixture(function: &mut Function) {
    let mut builder = Builder::new(function);
    let entry = builder.create_named_block("entry", vec![(IrType::Str, PhpType::Str)]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let source = builder.function().block(entry).unwrap().params[0];
    let assoc = builder
        .emit(
            Op::ConstBool,
            vec![],
            Some(Immediate::Bool(true)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .expect("assoc literal");
    let decoded = builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![source, assoc],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonDecode,
            ))),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
            RuntimeFnId::JsonDecode.effects(),
            None,
        )
        .expect("decoded cell");
    let slot = LocalSlotId::from_raw(0);
    let _ = builder.emit(
        Op::ReleaseLocalSlot,
        vec![],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder.emit(
        Op::StoreLocal,
        vec![decoded],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder.emit(
        Op::PushCallOperandOwner,
        vec![],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![decoded],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Count,
            ))),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
            RuntimeFnId::Count.effects(),
            None,
        )
        .expect("count result");
    let _ = builder.emit(
        Op::PopCallOperandOwner,
        vec![],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder.emit(
        Op::UnsetLocal,
        vec![],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = builder.emit(
        Op::Release,
        vec![decoded],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
}

/// The accepted graph retargets the count call and retires the decoder scaffolding.
#[test]
fn rewrites_count_of_json_decode() {
    let mut function = Function::new("count_decode".to_string(), IrType::I64, PhpType::Int);
    build_fixture(&mut function);
    assert!(sink(&mut function));
    let count = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::RuntimeCall)
        .expect("count call");
    assert_eq!(
        count.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::JsonDecodeCount
        )))
    );
    assert_eq!(count.operands.len(), 2);
    assert_eq!(count.result_type, IrType::I64);
    assert_eq!(count.result_php_type.codegen_repr(), PhpType::Int);
    assert!(!function.instructions.iter().any(|inst| {
        matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonDecode
            )))
        )
    }));
    assert!(!function
        .instructions
        .iter()
        .any(|inst| matches!(inst.op, Op::StoreLocal | Op::PushCallOperandOwner | Op::PopCallOperandOwner | Op::UnsetLocal | Op::ReleaseLocalSlot | Op::Release)));
}

/// A source released after the call is safe: the decode call itself becomes the count.
#[test]
fn accepts_a_source_released_after_the_count() {
    let mut function = Function::new("count_decode_temp".to_string(), IrType::I64, PhpType::Int);
    let source = {
        build_fixture(&mut function);
        function
            .blocks
            .first()
            .and_then(|block| block.params.first().copied())
            .expect("source parameter")
    };
    {
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(builder.function().blocks.first().expect("entry").id);
        let _ = builder.emit(
            Op::Release,
            vec![source],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }
    assert!(sink(&mut function));
    assert!(function.instructions.iter().any(|inst| {
        matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonDecodeCount
            )))
        )
    }));
    assert!(!function.instructions.iter().any(|inst| {
        matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonDecode
            )))
        )
    }));
}
