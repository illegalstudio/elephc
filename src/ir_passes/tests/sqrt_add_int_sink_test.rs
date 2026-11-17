//! Purpose:
//! Regression tests for the fused `(int)sqrt(int + int)` sink.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers the accepted add/sqrt/cast/release graph and conservative
//!   rejection when the boxed sum or the square root has any other observer.

use crate::ir::{
    Builder, DataPool, Function, Immediate, IrHeapKind, IrType, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::sqrt_add_int_sink::SqrtAddIntSink;
use crate::types::PhpType;

/// Runs the sink once with an unused literal pool.
fn sink(function: &mut Function) -> bool {
    SqrtAddIntSink.run(function, &mut DataPool::default())
}

/// Emits the boxed checked integer addition.
fn emit_checked_add(builder: &mut Builder<'_>, lhs: ValueId, rhs: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::ICheckedAdd,
            vec![lhs, rhs],
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
            Op::ICheckedAdd.default_effects(),
            None,
        )
        .expect("boxed checked add")
}

/// Emits the boxed float `sqrt()` runtime call.
fn emit_sqrt(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Sqrt,
            ))),
            IrType::F64,
            PhpType::Float,
            Ownership::NonHeap,
            RuntimeFnId::Sqrt.effects(),
            None,
        )
        .expect("boxed sqrt result")
}

/// Emits an explicit PHP integer cast of a float value.
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

/// Builds `(int)sqrt(lhs + rhs)` with the expected release scaffolding.
fn build_fixture(function: &mut Function) -> (ValueId, ValueId, ValueId) {
    let mut builder = Builder::new(function);
    let entry = builder.create_named_block(
        "entry",
        vec![(IrType::I64, PhpType::Int), (IrType::I64, PhpType::Int)],
    );
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let params = builder.function().block(entry).unwrap().params.clone();
    let sum = emit_checked_add(&mut builder, params[0], params[1]);
    let root = emit_sqrt(&mut builder, sum);
    let cast = emit_int_cast(&mut builder, root);
    let _ = builder.emit(
        Op::Release,
        vec![sum],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    (sum, root, cast)
}

/// Cast-only observation rewrites the fused call and removes the add/cast/release graph.
#[test]
fn rewrites_cast_observed_sqrt_add_to_raw_reducer() {
    let mut function = Function::new("sqrt_add_sink".to_string(), IrType::I64, PhpType::Int);
    let (sum, _root, _cast) = build_fixture(&mut function);
    assert!(sink(&mut function));
    let call = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::RuntimeCall)
        .expect("runtime call");
    assert_eq!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::SqrtAddInt
        )))
    );
    assert_eq!(call.operands.len(), 2);
    assert_eq!(call.result_type, IrType::I64);
    assert_eq!(call.result_php_type.codegen_repr(), PhpType::Int);
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::ICheckedAdd));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Cast));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Release));
    assert!(function.value(sum).is_some());
}

/// Any observer beyond the sqrt call keeps the boxed checked add.
#[test]
fn rejects_sum_with_an_extra_observer() {
    let mut function = Function::new("echo_sum".to_string(), IrType::I64, PhpType::Int);
    let (sum, _root, _cast) = build_fixture(&mut function);
    {
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(
            builder.function().blocks.first().expect("entry").id,
        );
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
    assert!(function.instructions.iter().any(|inst| inst.op == Op::ICheckedAdd));
    assert!(function.instructions.iter().any(|inst| {
        inst.immediate
            == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Sqrt,
            )))
    }));
}

/// A non-cast observer of the square root keeps both boxed operations.
#[test]
fn rejects_sqrt_with_a_non_cast_observer() {
    let mut function = Function::new("echo_sqrt".to_string(), IrType::I64, PhpType::Int);
    let (_sum, root, _cast) = build_fixture(&mut function);
    {
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(
            builder.function().blocks.first().expect("entry").id,
        );
        let _ = builder.emit(
            Op::EchoValue,
            vec![root],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }
    assert!(!sink(&mut function));
    assert!(!function.instructions.iter().any(|inst| {
        inst.immediate
            == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::SqrtAddInt,
            )))
    }));
}
