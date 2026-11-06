//! Purpose:
//! Regression tests for the raw `strlen(json_encode(...))` sink.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR covers the accepted encode/cast/str_len/release graph and conservative
//!   rejection when the encoded string has any observer other than the single length read.

use crate::ir::{
    Builder, DataPool, Function, Immediate, IrHeapKind, IrType, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::json_encode_len_sink::JsonEncodeLenSink;
use crate::types::PhpType;

/// Runs the sink once with an unused literal pool.
fn sink(function: &mut Function) -> bool {
    JsonEncodeLenSink.run(function, &mut DataPool::default())
}

/// Emits the boxed `json_encode()` runtime call.
fn emit_json_encode(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonEncode,
            ))),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
            RuntimeFnId::JsonEncode.effects(),
            None,
        )
        .expect("boxed json_encode result")
}

/// Emits the Mixed-to-string cast produced by the source-level `(string)` coercion.
fn emit_str_cast(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    builder
        .emit(
            Op::Cast,
            vec![value],
            Some(Immediate::CastTarget(IrType::Str)),
            IrType::Str,
            PhpType::Str,
            Ownership::MaybeOwned,
        )
        .expect("string cast result")
}

/// Emits a release of a heap-capable value.
fn emit_release(builder: &mut Builder<'_>, value: ValueId) {
    let _ = builder.emit(
        Op::Release,
        vec![value],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
}

/// Builds `strlen((string) json_encode($value))` with its release scaffolding.
fn build_fixture(function: &mut Function) -> (ValueId, ValueId) {
    let mut builder = Builder::new(function);
    let entry = builder.create_named_block("entry", vec![(IrType::Heap(IrHeapKind::Hash), PhpType::Mixed)]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let value = builder.function().block(entry).unwrap().params[0];
    let encoded = emit_json_encode(&mut builder, value);
    let text = emit_str_cast(&mut builder, encoded);
    emit_release(&mut builder, encoded);
    let _ = builder
        .emit(
            Op::StrLen,
            vec![text],
            None,
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .expect("string length");
    emit_release(&mut builder, text);
    (encoded, text)
}

/// Cast/length-only observation rewrites the encoder and removes the cast and Mixed release.
#[test]
fn rewrites_strlen_json_encode_to_raw_string() {
    let mut function = Function::new("json_len_sink".to_string(), IrType::I64, PhpType::Int);
    let (encoded, _text) = build_fixture(&mut function);
    assert!(sink(&mut function));
    let call = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::RuntimeCall)
        .expect("runtime call");
    assert_eq!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::JsonEncodeStr
        )))
    );
    assert_eq!(call.result_type, IrType::Str);
    assert_eq!(call.result_php_type.codegen_repr(), PhpType::Str);
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Cast));
    let releases = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::Release)
        .count();
    assert_eq!(releases, 1, "the raw string keeps exactly one release");
    assert!(function.values.get(encoded.as_raw() as usize).is_some());
}

/// Echoing the encoded string keeps the boxed call and the cast.
#[test]
fn rejects_json_encode_with_a_non_length_observer() {
    let mut function = Function::new("echo_json".to_string(), IrType::I64, PhpType::Int);
    let (_encoded, text) = build_fixture(&mut function);
    {
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(builder.function().blocks.first().expect("entry").id);
        let _ = builder.emit(
            Op::EchoValue,
            vec![text],
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
                RuntimeFnId::JsonEncode,
            )))
    }));
}
