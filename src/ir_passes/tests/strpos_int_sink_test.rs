//! Purpose:
//! Regression tests for the boxed `(int)strpos()` int-sink specialization.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers the accepted cast/release graph and conservative rejection
//!   when any other observer keeps the boxed `int|false` result alive.

use crate::ir::{
    Builder, DataPool, Function, Immediate, IrHeapKind, IrType, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::strpos_int_sink::StrposIntSink;
use crate::types::PhpType;

/// Runs the sink once with an unused literal pool.
fn sink(function: &mut Function) -> bool {
    StrposIntSink.run(function, &mut DataPool::default())
}

/// Emits the boxed `int|false` `strpos()` runtime call.
fn emit_strpos(builder: &mut Builder<'_>, haystack: ValueId, needle: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![haystack, needle],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Strpos,
            ))),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
            RuntimeFnId::Strpos.effects(),
            None,
        )
        .expect("boxed strpos result")
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

/// Cast-only observation rewrites the search and removes the cast/release scaffolding.
#[test]
fn rewrites_cast_observed_strpos_to_raw_integer() {
    let mut function = Function::new("strpos_sink".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![(IrType::Str, PhpType::Str), (IrType::Str, PhpType::Str)],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let params = builder.function().block(entry).unwrap().params.clone();
        let position = emit_strpos(&mut builder, params[0], params[1]);
        let _ = emit_int_cast(&mut builder, position);
        let _ = builder.emit(
            Op::Release,
            vec![position],
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
            RuntimeFnId::StrposInt
        )))
    );
    assert_eq!(call.result_type, IrType::I64);
    assert_eq!(call.result_php_type.codegen_repr(), PhpType::Int);
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Cast));
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Release));
}

/// Any observer beyond the int cast keeps the boxed result and the generic call.
#[test]
fn rejects_strpos_with_an_extra_observer() {
    let mut function = Function::new("echo_strpos".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![(IrType::Str, PhpType::Str), (IrType::Str, PhpType::Str)],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let params = builder.function().block(entry).unwrap().params.clone();
        let position = emit_strpos(&mut builder, params[0], params[1]);
        let _ = emit_int_cast(&mut builder, position);
        let _ = builder.emit(
            Op::EchoValue,
            vec![position],
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
                RuntimeFnId::Strpos,
            )))
    }));
}
