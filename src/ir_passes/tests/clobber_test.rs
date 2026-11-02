//! Purpose:
//! Regression tests for instruction-level caller-saved register clobber classification.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Pass-proven immutable concrete numeric loads are inline and volatile-safe.
//! - Packed reads and concrete scalar writes are inline and volatile-safe.
//! - Whole nested-record writes remain default-denied for both packed-address and `ptr<T>`
//!   sources because the pointer form calls a null guard and both forms run the copy loop.

use crate::ir::{
    Builder, DataId, Effects, Function, Immediate, IrHeapKind, IrType, LocalKind, Op, Ownership,
    Terminator,
};
use crate::ir_passes::clobber::instruction_is_volatile_safe;
use crate::types::PhpType;

/// Keeps packed resolved buffer stores call-clobbering while scalar pointer stores stay safe.
#[test]
fn buffer_record_copy_resolved_stores_are_not_volatile_safe() {
    for element in [PhpType::Packed("Record".into()), PhpType::Pointer(Some("Record".into()))] {
        let mut function = Function::new("copy".into(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![
                (IrType::Heap(IrHeapKind::Buffer), PhpType::Buffer(Box::new(element.clone()))),
                (IrType::I64, PhpType::Pointer(Some("Record".into()))),
            ]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let handle = builder.block_param(entry, 0);
            let source = builder.block_param(entry, 1);
            let descriptor = builder.emit(Op::BufferResolve, vec![handle], None,
                IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let index = builder.emit_const_i64(0);
            builder.emit(Op::BufferSetResolved, vec![descriptor, index, source], None,
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            builder.terminate(Terminator::Return { value: None });
        }
        let expected_safe = matches!(element, PhpType::Pointer(_));
        let store = function.instructions.iter()
            .find(|inst| inst.op == Op::BufferSetResolved).unwrap();
        assert_eq!(instruction_is_volatile_safe(&function, store), expected_safe, "{element:?}");
    }
}

/// Builds one named packed-field read whose receiver is a checked packed record address.
fn packed_read() -> Function {
    let mut function = Function::new("packed_read".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![(
                IrType::Heap(IrHeapKind::Object),
                PhpType::Packed("Body".to_string()),
            )],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let receiver = builder.block_param(entry, 0);
        let _ = builder.emit(
            Op::PropGet,
            vec![receiver],
            Some(Immediate::Data(DataId::from_raw(0))),
            IrType::F64,
            PhpType::Float,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    function
}

/// Builds one named packed-field write from a source with the requested concrete PHP type.
fn packed_write(source_type: PhpType) -> Function {
    let source_ir_type = IrType::from_php(&source_type);
    let mut function = Function::new("packed_write".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block(
            "entry",
            vec![
                (
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Packed("Body".to_string()),
                ),
                (source_ir_type, source_type),
            ],
        );
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let receiver = builder.block_param(entry, 0);
        let source = builder.block_param(entry, 1);
        let _ = builder.emit(
            Op::PropSet,
            vec![receiver, source],
            Some(Immediate::Data(DataId::from_raw(0))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Return { value: None });
    }
    function
}

/// Builds one local load with independently selected slot/result types and effects.
fn local_load(
    local_ir_type: IrType,
    local_php_type: PhpType,
    result_ir_type: IrType,
    result_php_type: PhpType,
    local_kind: LocalKind,
    pure: bool,
) -> Function {
    let mut function = Function::new(
        "local_load".to_string(),
        result_ir_type,
        result_php_type.clone(),
    );
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()),
            local_ir_type,
            local_php_type,
            local_kind,
        );
        let value = builder.emit_load_local(slot, result_ir_type, result_php_type);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    if pure {
        function.instructions[0].effects = Effects::PURE;
    }
    function
}

/// Pure loads with exact concrete integer or float local/result storage are volatile-safe.
#[test]
fn immutable_concrete_numeric_loads_are_volatile_safe() {
    for (ir_type, php_type) in [
        (IrType::I64, PhpType::Int),
        (IrType::F64, PhpType::Float),
    ] {
        let function = local_load(
            ir_type,
            php_type.clone(),
            ir_type,
            php_type.clone(),
            LocalKind::PhpLocal,
            true,
        );
        assert!(
            instruction_is_volatile_safe(&function, &function.instructions[0]),
            "source type {php_type:?}"
        );
    }
}

/// Effectful, ref-cell, Mixed, and mismatched local loads stay default-denied.
#[test]
fn unproven_or_non_concrete_local_loads_are_not_volatile_safe() {
    let cases = [
        local_load(
            IrType::F64,
            PhpType::Float,
            IrType::F64,
            PhpType::Float,
            LocalKind::PhpLocal,
            false,
        ),
        local_load(
            IrType::F64,
            PhpType::Float,
            IrType::F64,
            PhpType::Float,
            LocalKind::RefCell,
            true,
        ),
        local_load(
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
            true,
        ),
        local_load(
            IrType::I64,
            PhpType::Bool,
            IrType::I64,
            PhpType::Bool,
            LocalKind::PhpLocal,
            true,
        ),
        local_load(
            IrType::F64,
            PhpType::Float,
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
            true,
        ),
    ];
    for function in cases {
        assert!(!instruction_is_volatile_safe(
            &function,
            &function.instructions[0]
        ));
    }
}

/// A packed field read uses only its fixed result and documented scratch registers.
#[test]
fn packed_field_read_is_volatile_safe() {
    let function = packed_read();
    assert!(instruction_is_volatile_safe(&function, &function.instructions[0]));
}

/// Concrete integer, float, and boolean packed stores do not call or enter a returning side path.
#[test]
fn concrete_scalar_packed_field_stores_are_volatile_safe() {
    for source_type in [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::False] {
        let function = packed_write(source_type.clone());
        assert!(
            instruction_is_volatile_safe(&function, &function.instructions[0]),
            "source type {source_type:?}"
        );
    }
}

/// Packed and typed-pointer sources may select whole-record copy lowering and stay clobbers.
#[test]
fn nested_packed_record_stores_are_not_volatile_safe() {
    for source_type in [
        PhpType::Packed("Vec2".to_string()),
        PhpType::Pointer(Some("Vec2".to_string())),
    ] {
        let function = packed_write(source_type.clone());
        assert!(
            !instruction_is_volatile_safe(&function, &function.instructions[0]),
            "source type {source_type:?}"
        );
    }
}
