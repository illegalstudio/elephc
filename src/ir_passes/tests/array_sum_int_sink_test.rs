//! Purpose:
//! Regression tests for the boxed `array_sum()` int-sink specialization.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers the accepted cast/release graph and conservative rejection
//!   when any other observer keeps the boxed Mixed result alive.

use crate::ir::{
    Builder, DataPool, Function, Immediate, IrHeapKind, IrType, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::ir_passes::array_sum_int_sink::ArraySumIntSink;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Runs the sink once with an unused literal pool.
fn sink(function: &mut Function) -> bool {
    ArraySumIntSink.run(function, &mut DataPool::default())
}

/// Emits the boxed `array_sum(array<int>)` runtime call.
fn emit_array_sum(builder: &mut Builder<'_>, array: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![array],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArraySum,
            ))),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
            RuntimeFnId::ArraySum.effects(),
            None,
        )
        .expect("boxed array_sum result")
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
fn rewrites_cast_observed_array_sum_to_raw_reducer() {
    let mut function = Function::new("cast_sink".to_string(), IrType::I64, PhpType::Int);
    let array;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![(IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)))],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        array = builder.function().block(entry).unwrap().params[0];
        let sum = emit_array_sum(&mut builder, array);
        let cast = emit_int_cast(&mut builder, sum);
        let _ = builder.emit(
            Op::Release,
            vec![sum],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let _ = cast;
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
            RuntimeFnId::ArraySumInt
        )))
    );
    assert_eq!(call.result_type, IrType::I64);
    assert_eq!(call.result_php_type.codegen_repr(), PhpType::Int);
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Cast));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Release));
}

/// Any observer beyond the int cast keeps the boxed result and the generic call.
#[test]
fn rejects_array_sum_with_an_extra_observer() {
    let mut function = Function::new("echo_sink".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![(IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)))],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array = builder.function().block(entry).unwrap().params[0];
        let sum = emit_array_sum(&mut builder, array);
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
                RuntimeFnId::ArraySum,
            )))
    }));
}
