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
