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
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen_support::data_section::DataSection;
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

/// Verifies Windows releases UTF-8 argv while the aligned main frame is still active.
#[test]
fn windows_main_argv_cleanup_precedes_frame_restore() {
    let asm = empty_main_asm(Target::new(Platform::Windows, Arch::X86_64));
    let cleanup = asm
        .find("call __rt_sys_free_argv")
        .expect("Windows main must release its owned UTF-8 argv storage");
    let restore = asm[cleanup..]
        .find("pop rbp")
        .map(|offset| cleanup + offset)
        .expect("Windows main must restore its frame after argv cleanup");

    assert!(cleanup < restore, "argv cleanup ran after frame restore:\n{asm}");
}

/// Verifies non-Windows main epilogues do not acquire the Windows argv cleanup call.
#[test]
fn non_windows_main_epilogues_omit_argv_cleanup() {
    for target in [
        Target::new(Platform::Linux, Arch::X86_64),
        Target::new(Platform::Linux, Arch::AArch64),
    ] {
        let asm = empty_main_asm(target);
        assert!(!asm.contains("__rt_sys_free_argv"), "{target:?}:\n{asm}");
    }
}

/// Builds an empty top-level function for target-specific epilogue assertions.
fn empty_main_asm(target: Target) -> String {
    let mut module = Module::new(target);
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

    generate_user_asm_from_ir(&module, false, false).expect("empty main fixture should lower")
}

/// Verifies the Windows web process-entry stub reserves the mandatory MSx64
/// shadow area around its direct platform-ABI call into `elephc_web_run`.
#[test]
fn windows_web_entry_stub_reserves_native_shadow_space() {
    let asm = web_entry_stub_asm(Target::new(Platform::Windows, Arch::X86_64));
    let reserve = asm.find("sub rsp, 32").expect("Windows shadow-space reserve");
    let call = asm
        .find("\n    call elephc_web_run\n")
        .expect("web bridge entry call");
    let release = asm.find("add rsp, 32").expect("Windows shadow-space release");
    assert!(reserve < call && call < release, "{asm}");
    assert!(asm.contains("mov rcx"), "argc must use the first MSx64 register: {asm}");
    assert!(asm.contains("mov rdx"), "argv must use the second MSx64 register: {asm}");
}

/// Verifies Linux x86 retains the direct web-entry call without Windows shadow space.
#[test]
fn linux_web_entry_stub_remains_direct() {
    let asm = web_entry_stub_asm(Target::new(Platform::Linux, Arch::X86_64));
    assert!(asm.contains("call elephc_web_run"));
    assert!(!asm.contains("sub rsp, 32"));
    assert!(!asm.contains("add rsp, 32"));
}

/// Renders only the web process-entry stub for ABI-structure assertions.
fn web_entry_stub_asm(target: Target) -> String {
    let module = Module::new(target);
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let layout = layout_for_function(&function, target, false);
    let mut emitter = Emitter::new(target);
    let mut data = DataSection::new();
    let mut shared = SharedCodegenState::default();
    {
        let mut ctx = FunctionContext::new(
            &module,
            &function,
            &mut emitter,
            &mut data,
            &mut shared,
            layout,
            true,
            false,
            false,
            None,
        );
        emit_web_entry_stub(&mut ctx);
    }
    emitter.output()
}
