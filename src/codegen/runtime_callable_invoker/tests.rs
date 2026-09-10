//! Purpose:
//! Cross-assembles descriptor argument ownership and exception-boundary frames on every supported target.
//!
//! Called from:
//! - Focused compiler library tests with the explicit clang assembler gate enabled.
//!
//! Key details:
//! - Six string parameters exercise overflow staging, both container branches, and ledger growth paths.
//! - Native runtime-GC tests independently check aliases, normal cleanup, and exception unwinding.

use super::*;
use crate::codegen_support::platform::Target;
use std::process::Command;

/// Verifies expanded ARM64 invoker boundaries materialize far frame-slot addresses.
#[test]
fn arm64_invoker_boundary_uses_large_offset_frame_helpers() {
    let mut emitter = Emitter::new(Target::parse("linux-aarch64").unwrap());

    emit_invoker_exception_boundary_push(
        &mut emitter,
        INVOKER_BOUNDARY_BASE_OFFSET,
        "invoker_escape",
    );
    emit_invoker_exception_boundary_pop(&mut emitter, INVOKER_BOUNDARY_BASE_OFFSET);

    let output = emitter.output();
    assert!(INVOKER_BOUNDARY_BASE_OFFSET > 255);
    for offset in [
        INVOKER_BOUNDARY_BASE_OFFSET,
        INVOKER_BOUNDARY_BASE_OFFSET - 8,
        INVOKER_BOUNDARY_BASE_OFFSET - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    ] {
        assert!(output.contains(&format!("    sub x9, x29, #{}\n", offset)));
    }
    assert!(output.contains("    str x10, [x9]\n"));
    assert!(output.contains("    ldr x10, [x9]\n"));
    assert!(!output.contains("stur x10, [x29, #-3"));
    assert!(!output.contains("ldur x10, [x29, #-3"));
}

/// Assembles normal and eval-bounded descriptor invokers with direct and PIC references on all five targets.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn native_argument_owners_assemble_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("native-argument-owners-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let sig = FunctionSig {
        params: (0..6).map(|index| (format!("arg{index}"), PhpType::Str)).collect(),
        param_type_exprs: vec![None; 6], param_attributes: vec![Vec::new(); 6],
        defaults: vec![None; 6], return_type: PhpType::Str, declared_return: true,
        by_ref_return: false, ref_params: vec![false; 6], declared_params: vec![true; 6],
        variadic: None, deprecation: None,
    };
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        for pic in [false, true] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.pic_data_refs = pic;
            if emitter.target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
            emitter.raw(".text");
            let mut data = DataSection::new();
            for catch in [false, true] {
                let label = if catch { "__native_argument_eval_invoker" } else { "__native_argument_invoker" };
                let invoker = RuntimeCallableInvoker { label, sig: &sig, captures: &[], mbstring_operation: None };
                emit_runtime_callable_invoker_impl(&mut emitter, &mut data, &invoker, catch);
            }
            let source = directory.join(format!("{name}-{pic}.s"));
            let object = directory.join(format!("{name}-{pic}.o"));
            let target = emitter.target;
            std::fs::write(&source, format!("{}\n{}", emitter.output(), data.emit(target))).unwrap();
            let built = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(object).output().unwrap();
            assert!(built.status.success(), "{name}, PIC {pic}: {}", String::from_utf8_lossy(&built.stderr));
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
