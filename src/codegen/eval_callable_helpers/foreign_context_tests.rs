//! Purpose:
//! Checks delayed eval callback arena publication and callback-adapter entry protection.
//!
//! Called from:
//! - Focused compiler library tests, including the explicit clang assembler gate.
//!
//! Key details:
//! - Native-to-Rust calls use an additive five-argument C ABI, not descriptor ABI4 changes.
//! - Tests cover Mach-O/ELF mangling and both register conventions.

use super::*;
use crate::codegen::platform::{Arch, Target};

/// Emits the actual callback wrapper and invoker, sharing their real descriptor template.
fn emit_callback_entries(target: Target) -> (String, String) {
    let module = Module::new(target);
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    let mut data = DataSection::new();
    let descriptor = eval_dynamic_callable_descriptor(&mut data);
    emit_eval_callback_wrapper(&module, &mut emitter, &descriptor);
    emit_eval_dynamic_callable_invoker(&module, &mut emitter, &mut data);
    (emitter.output(), data.emit(target))
}

/// Publishes the caller arena before Rust runs and protects descriptor allocation on every ABI.
#[test]
fn delayed_eval_callback_entries_use_explicit_arena_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let (asm, _) = emit_callback_entries(target);
        assert!(asm.contains(&target.extern_symbol("__elephc_eval_callable_call_array_in_context_v1")), "{name}");
        assert!(!asm.contains(&format!("{}\n", target.extern_symbol("__elephc_eval_callable_call_array"))), "{name}: do not use the activation-free legacy callback entry");
        assert!(asm.contains(if target.arch == Arch::AArch64 { "mov x4, x28" } else { "mov r8, r14" }), "{name}: fifth C argument must be the current arena");
        let getter = asm.find("__elephc_eval_active_runtime_context").unwrap();
        let body = asm.find("__rt_eval_context_body_eval_wrap_callback:").unwrap();
        let alloc = asm.find("__rt_heap_alloc").unwrap();
        assert!(getter < body && body < alloc, "{name}: install arena before allocating the callback descriptor");
    }
}

/// Supplies the caller arena to dynamic callable validation on both register conventions.
#[test]
fn post_barrier_callable_descriptor_probe_publishes_arena_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut emitter = Emitter::new(target);
        let support = EvalCallableDescriptorSupport {
            argument_normalizer_needed: false, string_cases: Vec::new(),
            instance_array_cases: Vec::new(), static_array_cases: Vec::new(),
            object_cases: Vec::new(), dynamic_descriptor_label: Some("probe_descriptor".to_string()),
        };
        if target.arch == Arch::AArch64 {
            emit_aarch64_eval_dynamic_callable_descriptor(&module, &mut emitter, &support, "probe_fail", "x0");
        } else {
            emit_x86_64_eval_dynamic_callable_descriptor(&module, &mut emitter, &support, "probe_fail", "rax", 64);
        }
        let asm = emitter.output();
        let arena = asm.find(if target.arch == Arch::AArch64 { "mov x2, x28" } else { "mov rdx, r14" }).unwrap();
        let call = asm.find(&target.extern_symbol("__elephc_eval_is_callable_in_context_v1")).unwrap();
        assert!(arena < call, "{name}: the probe must not rely on a previous eval activation");
    }
}

/// Cross-assembles native/Rust callback entries and descriptor data for all five targets.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn delayed_eval_callback_entries_assemble_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("eval-delayed-context-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let (asm, data) = emit_callback_entries(Target::parse(name).unwrap());
        let source = directory.join(format!("{name}.s"));
        std::fs::write(&source, format!("{asm}\n{data}")).unwrap();
        let output = std::process::Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
            .arg("-o").arg(source.with_extension("o")).output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
    std::fs::remove_dir_all(directory).unwrap();
}
