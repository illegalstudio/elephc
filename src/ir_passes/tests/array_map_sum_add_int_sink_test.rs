//! Purpose:
//! Regression tests for the raw-int fused map+sum sink.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers the accepted cast/release graph and conservative rejection
//!   when another observer keeps the boxed Mixed result alive.

use crate::ir::{
    Builder, DataPool, Function, Immediate, IrHeapKind, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::ir_passes::array_map_sum_add_int_sink::ArrayMapSumAddIntSink;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Runs the sink once with an unused literal pool.
fn sink(function: &mut Function) -> bool {
    ArrayMapSumAddIntSink.run(function, &mut DataPool::default())
}

/// Emits the boxed fused map+sum runtime call.
fn emit_map_sum(builder: &mut Builder<'_>, array: ValueId, offset: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![array, offset],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArrayMapSumAdd,
            ))),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
            RuntimeFnId::ArrayMapSumAdd.effects(),
            None,
        )
        .expect("boxed map+sum result")
}

/// Emits an explicit PHP integer cast of a boxed Mixed value.
fn emit_int_cast(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    builder
        .emit(
            Op::Cast,
            vec![value],
            Some(Immediate::CastTarget(IrType::I64)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .expect("integer cast result")
}

/// Cast-only observation rewrites the call and removes the cast/release scaffolding.
#[test]
fn rewrites_cast_observed_map_sum_to_raw_kernel() {
    let mut function = Function::new("map_sink".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![(
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
            )],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array = builder.function().block(entry).unwrap().params[0];
        let offset = builder
            .emit(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(3)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("offset");
        let sum = emit_map_sum(&mut builder, array, offset);
        let _ = emit_int_cast(&mut builder, sum);
        let _ = builder.emit(
            Op::Release,
            vec![sum],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }
    assert!(sink(&mut function));
    let call = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::RuntimeCall)
        .expect("runtime call");
    assert_eq!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::ArrayMapSumAddInt
        )))
    );
    assert_eq!(call.result_type, IrType::I64);
    assert_eq!(call.result_php_type.codegen_repr(), PhpType::Int);
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Cast));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Release));
}

/// Any observer beyond the int cast keeps the boxed result and the generic call.
#[test]
fn rejects_map_sum_with_an_extra_observer() {
    let mut function = Function::new("map_echo".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![(
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
            )],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array = builder.function().block(entry).unwrap().params[0];
        let offset = builder
            .emit(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(3)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("offset");
        let sum = emit_map_sum(&mut builder, array, offset);
        let _ = emit_int_cast(&mut builder, sum);
        let _ = builder.emit(
            Op::EchoValue,
            vec![sum],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }
    assert!(!sink(&mut function));
    assert!(function.instructions.iter().any(|inst| {
        inst.immediate
            == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArrayMapSumAdd,
            )))
    }));
}
