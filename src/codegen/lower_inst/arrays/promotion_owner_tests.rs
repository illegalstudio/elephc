//! Purpose:
//! Guards indexed-to-hash promotion when a concrete load comes from late-widened Mixed storage.
//!
//! Called from:
//! - The indexed-array codegen unit suite.
//!
//! Key details:
//! - The outer Mixed cell must be retired before `ArrayToHash` consumes its retained child.
//! - Raw indexed-array slots still leave ownership consumption to `ArrayToHash` alone.

use crate::codegen::generate_user_asm_from_ir;
use crate::codegen::platform::Target;
use crate::ir::{Builder, Immediate, IrType, LocalKind, Module, Op, Ownership, Terminator};
use crate::types::PhpType;

/// A concrete array loaded from a final Mixed slot releases the superseded outer cell first.
#[test]
fn array_to_hash_retires_late_widened_mixed_source_on_every_target() {
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = Target::parse(name).unwrap();
        let module = late_widened_array_to_hash_module(target);
        let asm = generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        // User assembly does not enable EIR annotation comments. This fixture contains one
        // conversion only, so ordering the two unique helper calls across the function is the
        // stable backend contract.
        let conversion = asm.as_str();
        let release = conversion
            .find("__rt_decref_mixed")
            .unwrap_or_else(|| panic!("{name}: the superseded Mixed owner was not retired\n{asm}"));
        let consume = conversion
            .find("__rt_heap_kind")
            .unwrap_or_else(|| panic!("{name}: missing ArrayToHash runtime dispatch\n{asm}"));
        assert!(
            release < consume,
            "{name}: the outer Mixed owner must be retired before its retained child is consumed\n{asm}"
        );
    }
}

/// Builds the ownership shape produced when a loop representation contract is widened late.
fn late_widened_array_to_hash_module(target: Target) -> Module {
    let mut module = Module::new(target);
    let mut main = crate::ir::Function::new("main".into(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    let slot = main.add_local(
        Some("array".into()),
        IrType::from_php(&PhpType::Mixed),
        PhpType::Mixed,
        LocalKind::PhpLocal,
    );
    {
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array_ty = PhpType::Array(Box::new(PhpType::Never));
        let array = builder
            .emit(
                Op::ArrayNew,
                Vec::new(),
                Some(Immediate::Capacity(0)),
                IrType::from_php(&array_ty),
                array_ty.clone(),
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        builder.emit_store_local(slot, array);
        let loaded = builder.emit_load_local(slot, IrType::from_php(&array_ty), array_ty);
        let hash_ty = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        };
        let hash = builder
            .emit(
                Op::ArrayToHash,
                vec![loaded],
                None,
                IrType::from_php(&hash_ty),
                hash_ty,
                Ownership::Owned,
            )
            .expect("array_to_hash produces a value");
        builder.emit_store_local(slot, hash);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    module
}

/// A borrowed source that is already hash-backed hands `ArrayToHash` a retained result even when
/// the requested entry type is not Mixed.
///
/// Every other arm returns an owner (a fresh hash, or a retained one for a Mixed result), and the
/// IR declares the result owned; the non-Mixed reuse arm returned the source's own pointer
/// without a reference, so the result's release dropped a reference the local still counted on.
#[test]
fn array_to_hash_retains_reused_hash_for_a_borrowed_source_on_every_target() {
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = Target::parse(name).unwrap();
        let module = borrowed_typed_array_to_hash_module(target);
        let asm = generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let label_start = |prefix: &str| {
            asm.lines()
                .position(|line| line.contains(prefix) && line.trim_end().ends_with(':'))
                .unwrap_or_else(|| panic!("{name}: missing {prefix} label\n{asm}"))
        };
        let reuse = label_start("array_to_hash_already_hash");
        let convert = label_start("array_to_hash_convert");
        assert!(reuse < convert, "{name}: unexpected arm order\n{asm}");
        let arm: Vec<&str> = asm.lines().skip(reuse).take(convert - reuse).collect();
        assert!(
            arm.iter().any(|line| line.contains("__rt_incref")),
            "{name}: the reused hash must be retained for a borrowed source\n{}",
            arm.join("\n")
        );
    }
}

/// Builds `$dst = (array<int, int>) $src` where `$src` is a borrowed `array<int>` local load.
fn borrowed_typed_array_to_hash_module(target: Target) -> Module {
    let mut module = Module::new(target);
    let mut main = crate::ir::Function::new("main".into(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    let array_ty = PhpType::Array(Box::new(PhpType::Int));
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(PhpType::Int),
    };
    let source = main.add_local(
        Some("src".into()),
        IrType::from_php(&array_ty),
        array_ty.clone(),
        LocalKind::PhpLocal,
    );
    let target_slot = main.add_local(
        Some("dst".into()),
        IrType::from_php(&hash_ty),
        hash_ty.clone(),
        LocalKind::PhpLocal,
    );
    {
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array = builder
            .emit(
                Op::ArrayNew,
                Vec::new(),
                Some(Immediate::Capacity(4)),
                IrType::from_php(&array_ty),
                array_ty.clone(),
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        builder.emit_store_local(source, array);
        let loaded = builder
            .emit(
                Op::LoadLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(source)),
                IrType::from_php(&array_ty),
                array_ty.clone(),
                Ownership::Borrowed,
            )
            .expect("load_local produces a value");
        let hash = builder
            .emit(
                Op::ArrayToHash,
                vec![loaded],
                None,
                IrType::from_php(&hash_ty),
                hash_ty,
                Ownership::Owned,
            )
            .expect("array_to_hash produces a value");
        builder.emit_store_local(target_slot, hash);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    module
}
