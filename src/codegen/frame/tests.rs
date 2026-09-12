//! Purpose:
//! Unit tests for callable-frame parameter setup and ownership retention.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures inspect both supported ABIs so runtime calls never precede later argument saves.

use super::*;
use crate::codegen::generate_user_asm_from_ir;
use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{Builder, FunctionParam, IrType, Module, Terminator};
use crate::types::FunctionSig;

/// Descriptor-only backtrace reachability is carried by the frontend's hidden frame snapshot.
#[test]
fn hidden_argument_snapshot_enables_backtrace_activations_without_a_core_instruction() {
    let target = Target::new(Platform::Linux, Arch::X86_64);
    let mut module = Module::new(target);
    let mut function = Function::new(
        "dynamic_backtrace_frame".to_string(),
        IrType::Void,
        PhpType::Void,
    );
    function.params = vec![
        FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        },
        FunctionParam {
            name: crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
            ir_type: IrType::Heap(crate::ir::HeapKind::Array),
            php_type: PhpType::Array(Box::new(PhpType::Mixed)),
            by_ref: false,
            variadic: true,
        },
    ];
    // Public metadata may omit compiler-owned ABI parameters. Frame publication must use the
    // physical EIR layout, which remains authoritative for the reader callback.
    function.signature = Some(FunctionSig {
        params: vec![("value".to_string(), PhpType::Int)],
        param_type_exprs: vec![None],
        param_attributes: vec![Vec::new()],
        defaults: vec![None],
        return_type: PhpType::Void,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false],
        declared_params: vec![true],
        variadic: None,
        deprecation: None,
    });
    module.add_function(function);

    let backtrace_enabled = module_uses_backtrace(&module);
    assert!(backtrace_enabled);
    let layout = layout_for_function(
        &module.functions[0],
        target,
        false,
        true,
        backtrace_enabled,
    );
    assert!(layout.backtrace_activation);
    assert!(layout.exception_activation_offset.is_some());
}

/// Both dynamic constructor opcodes reserve the hand-used receiver register on every target.
#[test]
fn dynamic_constructor_frames_preserve_the_nested_receiver_register() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for op in [Op::DynamicObjectNew, Op::DynamicObjectNewMixed] {
            let mut function = Function::new("dynamic_constructor_frame".into(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                // Frame analysis needs only the opcode, not candidate metadata or emission.
                builder.emit(op, Vec::new(), None, IrType::Void, PhpType::Void, crate::ir::Ownership::NonHeap);
                builder.terminate(Terminator::Return { value: None });
            }
            for regalloc in [false, true] {
                let layout = layout_for_function(&function, target, regalloc, false, false);
                let register = nested_call_reg_name(target.arch);
                assert_eq!(layout.callee_saved_offsets.iter().filter(|(saved, _)| *saved == register).count(),
                    1, "{name}: {op:?}, register allocation {regalloc}");
            }
        }
    }
}

/// Verifies AArch64 saves a later Mixed argument before retaining an earlier string.
#[test]
fn aarch64_prologue_saves_all_parameters_before_runtime_calls() {
    let asm = owned_string_then_mixed_prologue_asm(Target::new(
        Platform::Linux,
        Arch::AArch64,
    ));
    let later_param = asm
        .find("param $value from x2")
        .expect("AArch64 fixture should receive the later Mixed parameter in x2");
    let persist = asm
        .find("bl __rt_str_persist")
        .expect("owned string parameter should be persisted");

    assert!(later_param < persist, "later parameter was saved after retain:\n{asm}");
    assert!(asm[later_param..persist].contains("x2, [x29"), "{asm}");
}

/// Verifies x86_64 saves a later Mixed argument before retaining an earlier string.
#[test]
fn x86_64_prologue_saves_all_parameters_before_runtime_calls() {
    let asm = owned_string_then_mixed_prologue_asm(Target::new(
        Platform::Linux,
        Arch::X86_64,
    ));
    let later_param = asm
        .find("param $value from rdx")
        .expect("x86_64 fixture should receive the later Mixed parameter in rdx");
    let persist = asm
        .find("call __rt_str_persist")
        .expect("owned string parameter should be persisted");

    assert!(later_param < persist, "later parameter was saved after retain:\n{asm}");
    assert!(asm[later_param..persist].contains("rdx"), "{asm}");
}

/// Verifies the string retention helper emits one persist call on each supported ABI shape.
#[test]
fn owned_string_parameter_is_persisted_once() {
    for (target, call) in [
        (
            Target::new(Platform::Linux, Arch::AArch64),
            "bl __rt_str_persist",
        ),
        (
            Target::new(Platform::Linux, Arch::X86_64),
            "call __rt_str_persist",
        ),
    ] {
        let mut emitter = Emitter::new(target);
        retain_owned_parameter_local(&mut emitter, 16, &PhpType::Str);
        let asm = emitter.output();

        assert_eq!(asm.matches(call).count(), 1, "{asm}");
    }
}

/// Verifies callable prologues use only the stack-pointer guard and do not add
/// process-global byte accounting calls on function entry or return.
#[test]
fn callable_frames_do_not_emit_a_second_stack_budget_guard() {
    for target in [
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        let asm = owned_string_then_mixed_prologue_asm(target);
        assert!(asm.contains("call-stack overflow guard"), "{target:?}: {asm}");
        assert!(!asm.contains("recursion_stack_bytes"), "{target:?}: {asm}");
    }
}

/// Builds a callable with an owned string parameter followed by a borrowed Mixed parameter.
fn owned_string_then_mixed_prologue_asm(target: Target) -> String {
    let mut module = Module::new(target);
    let mut function = Function::new(
        "prologue_parameter_fixture".to_string(),
        IrType::Void,
        PhpType::Void,
    );
    function.params.push(FunctionParam {
        name: "label".to_string(),
        ir_type: IrType::Str,
        php_type: PhpType::Str,
        by_ref: false,
        variadic: false,
    });
    function.params.push(FunctionParam {
        name: "value".to_string(),
        ir_type: IrType::Heap(crate::ir::IrHeapKind::Mixed),
        php_type: PhpType::Mixed,
        by_ref: false,
        variadic: false,
    });
    let label_slot = function.add_local(
        Some("label".to_string()),
        IrType::Str,
        PhpType::Str,
        LocalKind::PhpLocal,
    );
    function.add_local(
        Some("value".to_string()),
        IrType::Heap(crate::ir::IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::PhpLocal,
    );
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let label = builder.emit_load_local(label_slot, IrType::Str, PhpType::Str);
        builder.emit_store_local(label_slot, label);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(function);

    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    {
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);

    generate_user_asm_from_ir(&module, false, false)
        .expect("parameter-prologue fixture should lower")
}
