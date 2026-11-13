//! Purpose:
//! Guards reference publication of acquired increments after an owned branch merge.
//!
//! Called from:
//! - The backend unit suite on every supported target.
//!
//! Key details:
//! - Tiny EIR modules isolate the ABI boundary without lowering an opaque-eval catalog.
//! - Executable ownership coverage remains in the XML composed-helper heap regression.

use crate::codegen::{generate_user_asm_from_ir, platform::Target};
use crate::ir::{Builder, Function, FunctionParam, Immediate, IrHeapKind, IrType, LocalKind,
    Module, Op, Ownership, Terminator};
use crate::types::PhpType;

/// A transferred increment retires the previous cell owner even when its pointer is unchanged.
#[test]
fn merged_reference_increment_retires_previous_owner_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let assembly = merged_increment_assembly(Target::parse(name).unwrap());
        let body = assembly.split_once("@fn name=reference_increment_merge_fixture ").unwrap().1
            .split_once("@endfn name=reference_increment_merge_fixture").unwrap().0;
        assert!(body.contains("__rt_mixed_inc_dec"), "{name}: missing increment: {body}");
        assert!(body.contains("__rt_cleanup_preserve_exception"), "{name}: missing previous-owner retirement: {body}");
        assert!(!body.contains("ref_cell_store_same_pointer"),
            "{name}: pointer equality must not suppress retirement of an acquired owner: {body}");
    }
}

/// Builds the branch-temporary shape used by dynamic internal-object arithmetic.
fn merged_increment_assembly(target: Target) -> String {
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let mut module = Module::new(target);
    let mut function = Function::new("reference_increment_merge_fixture".to_string(),
        IrType::Void, PhpType::Void);
    function.params = vec![
        FunctionParam { name: "value".to_string(), ir_type: mixed, php_type: PhpType::Mixed,
            by_ref: true, variadic: false },
        FunctionParam { name: "increment".to_string(), ir_type: IrType::I64,
            php_type: PhpType::Bool, by_ref: false, variadic: false },
    ];
    let source = function.add_local(Some("value".to_string()), mixed,
        PhpType::Mixed, LocalKind::PhpLocal);
    let condition = function.add_local(Some("increment".to_string()), IrType::I64,
        PhpType::Bool, LocalKind::PhpLocal);
    let temporary = function.add_local(Some("merged".to_string()), mixed,
        PhpType::Mixed, LocalKind::OwnedTemp);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let increment = builder.create_named_block("increment", vec![]);
        let replacement = builder.create_named_block("replacement", vec![]);
        let merge = builder.create_named_block("merge", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let old = builder.emit(Op::LoadRefCell, vec![], Some(Immediate::LocalSlot(source)),
            mixed, PhpType::Mixed, Ownership::Borrowed).unwrap();
        let condition = builder.emit_load_local(condition, IrType::I64, PhpType::Bool);
        builder.terminate(Terminator::CondBr { cond: condition, then_target: increment,
            then_args: vec![], else_target: replacement, else_args: vec![] });
        builder.position_at_end(increment);
        let updated = builder.emit(Op::StrIncDec, vec![old], Some(Immediate::I64(1)),
            mixed, PhpType::Mixed, Ownership::Owned).unwrap();
        builder.emit(Op::StoreLocal, vec![updated], Some(Immediate::LocalSlot(temporary)),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
        builder.terminate(Terminator::Br { target: merge, args: vec![] });
        builder.position_at_end(replacement);
        let scalar = builder.emit_const_i64(7);
        let boxed = builder.emit(Op::MixedBox, vec![scalar], None, mixed,
            PhpType::Mixed, Ownership::Owned).unwrap();
        builder.emit(Op::StoreLocal, vec![boxed], Some(Immediate::LocalSlot(temporary)),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
        builder.terminate(Terminator::Br { target: merge, args: vec![] });
        builder.position_at_end(merge);
        let result = builder.emit(Op::LoadLocal, vec![], Some(Immediate::LocalSlot(temporary)),
            mixed, PhpType::Mixed, Ownership::Owned).unwrap();
        builder.emit(Op::UnsetLocal, vec![], Some(Immediate::LocalSlot(temporary)),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
        let acquired = builder.emit(Op::Acquire, vec![result], None, mixed,
            PhpType::Mixed, Ownership::Owned).unwrap();
        builder.emit(Op::StoreRefCell, vec![acquired], Some(Immediate::LocalSlot(source)),
            IrType::Void, PhpType::Mixed, Ownership::NonHeap);
        builder.emit(Op::Release, vec![result], None, IrType::Void,
            PhpType::Void, Ownership::NonHeap);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(function);
    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    {
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    crate::ir::validate_module(&module).expect("valid reference increment fixture");
    generate_user_asm_from_ir(&module, false, false).expect("lower reference increment fixture")
}
