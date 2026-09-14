//! Purpose:
//! Unit tests for callable-frame parameter setup and ownership retention.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures inspect both supported ABIs so runtime calls never precede later argument saves.
//! - Eval fixtures cover implicit process-superglobal ownership across all supported targets.

use super::*;
use crate::codegen::generate_user_asm_from_ir;
use crate::codegen::platform::{AppleVariant, Arch, Platform, Target};
use crate::ir::{Builder, FunctionParam, IrType, Module, Terminator};

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

/// Verifies a borrowed Mixed parameter never masquerades as an owned return slot.
#[test]
fn borrowed_mixed_parameter_return_publishes_borrowed_status() {
    for (target, borrowed_status) in [
        (
            Target::new(Platform::Linux, Arch::AArch64),
            "mov x15, xzr",
        ),
        (
            Target::new(Platform::Linux, Arch::X86_64),
            "xor r11d, r11d",
        ),
    ] {
        let asm = owned_string_then_mixed_prologue_asm(target);
        let function_start = asm
            .find("prologue_parameter_fixture")
            .expect("fixture function label should be emitted");
        let function_return = asm[function_start..]
            .find("\n    ret\n")
            .map(|offset| function_start + offset)
            .expect("fixture function should return");
        let function_asm = &asm[function_start..function_return];

        assert!(function_asm.contains(borrowed_status), "{target:?}: {function_asm}");
    }
}

/// Verifies normal exit releases the implicit eval `$argv` global exactly once.
#[test]
fn implicit_eval_argv_global_is_released_on_all_targets() {
    for target in [
        Target::new(Platform::Linux, Arch::X86_64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
    ] {
        for visible_process_args in [false, true] {
            let asm = eval_argv_cleanup_asm(target, visible_process_args, false);
            assert_eq!(
                asm.matches("build global $argv array from OS argv").count(),
                1,
                "{target:?}, visible={visible_process_args}: {asm}"
            );
            assert_eq!(
                asm.matches("epilogue cleanup global $argv").count(),
                1,
                "{target:?}, visible={visible_process_args}: {asm}"
            );
            assert_eval_argv_cleanup(target, &asm);

            if visible_process_args {
                assert!(asm.contains("build $argv array from OS argv"), "{target:?}: {asm}");
                assert!(asm.contains("epilogue cleanup $argv"), "{target:?}: {asm}");
            }
        }
    }
}

/// Verifies an explicitly interned `$argv` global does not duplicate implicit eval cleanup.
#[test]
fn explicit_eval_argv_global_is_released_once() {
    let asm = eval_argv_cleanup_asm(
        Target::new(Platform::Linux, Arch::AArch64),
        true,
        true,
    );

    assert_eq!(asm.matches("epilogue cleanup global $argv").count(), 1, "{asm}");
    assert_eval_argv_cleanup(Target::new(Platform::Linux, Arch::AArch64), &asm);
}

/// Verifies widening the local eval `$argv` to Mixed transfers its fresh Array owner.
#[test]
fn mixed_eval_argv_transfers_fresh_array_owner_on_all_targets() {
    for target in [
        Target::new(Platform::Linux, Arch::X86_64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
    ] {
        let asm = eval_argv_ownership_asm(target, PhpType::Mixed);
        let local_init = asm
            .split_once("build $argv array from OS argv")
            .map(|(_, local_init)| local_init)
            .expect("Mixed eval argv fixture should initialize the local array");
        let local_init = local_init
            .split_once("epilogue + exit(0)")
            .map_or(local_init, |(local_init, _)| local_init);
        let box_call = match target.arch {
            Arch::AArch64 => "bl __rt_mixed_from_value",
            Arch::X86_64 => "call __rt_mixed_from_value",
        };
        let release_call = match target.arch {
            Arch::AArch64 => "bl __rt_decref_array",
            Arch::X86_64 => "call __rt_decref_array",
        };
        let box_at = local_init
            .find(box_call)
            .unwrap_or_else(|| panic!("{target:?}: missing Mixed argv box:\n{local_init}"));
        let release_at = local_init
            .find(release_call)
            .unwrap_or_else(|| panic!("{target:?}: missing transferred Array release:\n{local_init}"));

        assert!(box_at < release_at, "{target:?}: {local_init}");
    }
}

/// Verifies the global cleanup releases an Array owner and clears its symbol slot.
fn assert_eval_argv_cleanup(target: Target, asm: &str) {
    let cleanup = asm
        .split_once("epilogue cleanup global $argv")
        .map(|(_, cleanup)| cleanup)
        .expect("eval argv cleanup marker should be emitted");
    let cleanup = cleanup
        .split_once("__rt_mbstring_release_catalog")
        .map_or(cleanup, |(cleanup, _)| cleanup);

    assert!(cleanup.contains("__rt_decref_array"), "{target:?}: {cleanup}");
    assert!(!cleanup.contains("__rt_decref_mixed"), "{target:?}: {cleanup}");
    assert!(cleanup.contains("_eir_global_argv"), "{target:?}: {cleanup}");
    match target.arch {
        Arch::AArch64 => assert!(cleanup.contains("str xzr, [x9]"), "{target:?}: {cleanup}"),
        Arch::X86_64 => assert!(
            cleanup.contains("mov QWORD PTR [rip + _eir_global_argv], 0"),
            "{target:?}: {cleanup}"
        ),
    }
}

/// Builds an eval-capable main function with optional PHP-visible process arguments.
fn eval_argv_cleanup_asm(
    target: Target,
    visible_process_args: bool,
    explicit_argv_global: bool,
) -> String {
    let local_ty = visible_process_args.then(|| PhpType::Array(Box::new(PhpType::Str)));
    eval_argv_cleanup_asm_with_local_type(target, local_ty, explicit_argv_global)
}

/// Builds an eval-capable main function with a specific local `$argv` storage type.
fn eval_argv_ownership_asm(target: Target, argv_ty: PhpType) -> String {
    eval_argv_cleanup_asm_with_local_type(target, Some(argv_ty), false)
}

/// Builds the shared eval `$argv` assembly fixture.
fn eval_argv_cleanup_asm_with_local_type(
    target: Target,
    argv_ty: Option<PhpType>,
    explicit_argv_global: bool,
) -> String {
    let mut module = Module::new(target);
    module.required_runtime_features.eval_bridge = true;
    if explicit_argv_global {
        module.data.intern_global_name("argv");
    }

    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    main.add_local(
        Some("__eir_eval_scope".to_string()),
        IrType::I64,
        PhpType::Int,
        LocalKind::EvalScope,
    );
    if let Some(argv_ty) = argv_ty {
        main.add_local(
            Some("argc".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        main.add_local(
            Some("argv".to_string()),
            if argv_ty.codegen_repr() == PhpType::Mixed {
                IrType::Heap(crate::ir::IrHeapKind::Mixed)
            } else {
                IrType::Heap(crate::ir::IrHeapKind::Array)
            },
            argv_ty,
            LocalKind::PhpLocal,
        );
    }
    {
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);

    generate_user_asm_from_ir(&module, false, false)
        .expect("eval argv cleanup fixture should lower")
}

/// Builds a callable with an owned string parameter followed by a borrowed Mixed parameter.
fn owned_string_then_mixed_prologue_asm(target: Target) -> String {
    let mut module = Module::new(target);
    let mut function = Function::new(
        "prologue_parameter_fixture".to_string(),
        IrType::Heap(crate::ir::IrHeapKind::Mixed),
        PhpType::Mixed,
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
    let value_slot = function.add_local(
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
        let value = builder.emit_load_local(
            value_slot,
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        builder.terminate(Terminator::Return { value: Some(value) });
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
