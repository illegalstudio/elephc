//! Purpose:
//! Checks eval closure rebinding selects a context-aware hook without changing legacy ABI.
//!
//! Called from:
//! - The focused native closure runtime library tests.
//!
//! Key details:
//! - The v2 hook receives the actual arena; the v1 callback keeps its four-argument contract.
//! - Both architectures must retain the native capture-shape path.

use super::*;
use crate::codegen_support::platform::Target;

/// Prefers the explicitly versioned context hook and retains a separate legacy fallback.
#[test]
fn post_barrier_closure_bind_selects_context_hook_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit_closure_bind(&mut emitter);
        let asm = emitter.output();
        let context_hook = asm.find("_elephc_eval_closure_bind_ctx_fn").expect("context-aware hook slot");
        let legacy_hook = asm.find("_elephc_eval_closure_bind_fn").unwrap();
        assert!(context_hook < legacy_hook, "{name}: prefer v2 without reinterpreting v1");
        assert!(asm.contains(if target.arch == Arch::AArch64 { "mov x4, x28" } else { "mov r8, r14" }), "{name}: fifth argument must be the actual arena");
        assert!(asm.contains("__rt_closure_bind_native:"), "{name}");
        assert!(!asm.contains("__elephc_eval_active_runtime_context"), "{name}: native closure binding must not depend on Magician");
    }
}

/// Cross-assembles both rebinding ABI branches on every supported object format.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn post_barrier_closure_bind_context_branches_assemble_on_all_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("closure-bind-context-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        emit_closure_bind(&mut emitter);
        let source = directory.join(format!("{name}.s"));
        std::fs::write(&source, emitter.output()).unwrap();
        let compiled = std::process::Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
            .arg("-o").arg(source.with_extension("o")).output().unwrap();
        assert!(compiled.status.success(), "{name}: {}", String::from_utf8_lossy(&compiled.stderr));
    }
    std::fs::remove_dir_all(directory).unwrap();
}
