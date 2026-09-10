//! Purpose:
//! Guards element-reference address materialization against large-frame scratch collisions.
//!
//! Called from:
//! - The indexed-array codegen unit suite.
//!
//! Key details:
//! - Stack-only allocation forces both operands beyond AArch64's unscaled load range.
//! - One-word and string-pair elements retain the same address contract on all targets.

use crate::codegen::{generate_user_asm_from_ir_with_options, Emit, Instrumentation, WebIsolation};
use crate::codegen::platform::{Arch, Target};
use crate::ir::{Builder, Function, FunctionParam, IrType, LocalKind, Module, Op, Ownership, Terminator};
use crate::types::PhpType;

/// Large-frame index loads cannot overwrite the already materialized array base on AArch64.
#[test]
fn spilled_element_address_operands_preserve_the_array_base_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for element in [PhpType::Int, PhpType::Str] {
            let module = large_frame_element_address_module(target, element);
            let asm = generate_user_asm_from_ir_with_options(
                &module, false, false, Instrumentation::Off, false, false, false,
                Emit::Executable, &Default::default(), false, false, WebIsolation::Worker,
            ).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            if target.arch == Arch::AArch64 {
                let (prefix, _) = asm.split_once("add x0, x9, #24").unwrap();
                let index = prefix.rfind("ldr x10, [x9]").expect("index must require a large-frame load");
                let base = prefix.rfind("ldr x9, [x9]").expect("array must require a large-frame load");
                assert!(index < base, "{name}: array base was clobbered by the index load\n{asm}");
            } else {
                assert!(asm.contains("[r10 + 24 + r11"), "{name}: preserve x86_64 element addressing");
            }
        }
    }
}

/// Builds a minimal address operation whose SSA spill slots follow forty frame locals.
fn large_frame_element_address_module(target: Target, element: PhpType) -> Module {
    let mut module = Module::new(target);
    let mut function = Function::new("large_frame_element_address".into(), IrType::Void, PhpType::Void);
    let array_type = PhpType::Array(Box::new(element));
    for (name, php_type) in [("array", array_type.clone()), ("index", PhpType::Int)] {
        let ir_type = IrType::from_php(&php_type);
        function.params.push(FunctionParam {
            name: name.into(), ir_type, php_type: php_type.clone(), by_ref: false, variadic: false,
        });
        function.add_local(Some(name.into()), ir_type, php_type, LocalKind::PhpLocal);
    }
    for index in 0..40 {
        function.add_local(Some(format!("padding{index}")), IrType::I64, PhpType::Int, LocalKind::HiddenTemp);
    }
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let array = builder.emit_load_local(crate::ir::LocalSlotId::from_raw(0), IrType::from_php(&array_type), array_type);
        let index = builder.emit_load_local(crate::ir::LocalSlotId::from_raw(1), IrType::I64, PhpType::Int);
        builder.emit(Op::ArrayElemAddr, vec![array, index], None, IrType::I64, PhpType::Pointer(None), Ownership::NonHeap);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(function);
    let mut main = Function::new("main".into(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    {
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    module
}
