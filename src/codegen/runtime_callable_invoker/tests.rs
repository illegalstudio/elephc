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

/// Selects borrowed promotion or owned transfer from the per-return ABI status on every target.
#[test]
fn mixed_invoker_returns_follow_runtime_ownership_on_all_targets() {
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        let mut ctx = InvokerEmitContext::new("mixed_owner_invoker", target);
        emit_boxed_invoker_return(&mut emitter, &PhpType::Mixed, false, &mut ctx);
        let arch = emitter.target.arch;
        let output = emitter.output();
        let retain = output.find("__rt_incref").expect("borrowed path must retain");
        let owned_label = format!(
            "{}mixed_owner_invoker_return_owned_0:",
            target.platform.local_label_prefix()
        );
        let owned = output
            .find(&owned_label)
            .expect("owned path label must be emitted");
        assert!(retain < owned, "{name}: borrowed promotion must precede owned transfer");
        match arch {
            Arch::AArch64 => assert!(output.contains("cbnz x15,"), "{name}"),
            Arch::X86_64 => {
                assert!(output.contains("test r11, r11"), "{name}");
                assert!(output.contains("jne "), "{name}");
            }
        }
    }
}

/// Publishes both ownership states in the target-specific internal return register.
#[test]
fn eir_return_ownership_status_is_target_aware() {
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        super::super::return_ownership::emit_status(&mut emitter, false);
        super::super::return_ownership::emit_status(&mut emitter, true);
        let arch = emitter.target.arch;
        let output = emitter.output();
        match arch {
            Arch::AArch64 => {
                assert!(output.contains("mov x15, xzr"), "{name}");
                assert!(output.contains("mov x15, #1"), "{name}");
            }
            Arch::X86_64 => {
                assert!(output.contains("xor r11d, r11d"), "{name}");
                assert!(output.contains("mov r11d, 1"), "{name}");
            }
        }
    }
}

/// Keeps by-reference returns on the existing storage-pointer path.
#[test]
fn by_ref_invoker_returns_ignore_the_internal_ownership_status() {
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        let mut ctx = InvokerEmitContext::new("by_ref_invoker", target);
        emit_boxed_invoker_return(&mut emitter, &PhpType::Mixed, true, &mut ctx);
        let output = emitter.output();
        assert!(!output.contains("return_owned"), "{name}: {output}");
        assert!(!output.contains("__rt_incref"), "{name}: {output}");
        assert!(!output.contains("x15"), "{name}: {output}");
        assert!(!output.contains("r11"), "{name}: {output}");
    }
}

/// Consumes a compiled string marker before persistence or concat restoration can clobber it.
#[test]
fn by_value_string_returns_persist_only_the_borrowed_path() {
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        let mut ctx = InvokerEmitContext::new("string_owner_invoker", target);
        restore_concat_offset_after_nested_call(&mut emitter, &PhpType::Str, false, &mut ctx);
        let arch = emitter.target.arch;
        let output = emitter.output();
        let branch = match arch {
            Arch::AArch64 => output.find("cbnz x15,").expect("AArch64 marker branch"),
            Arch::X86_64 => output.find("test r11, r11").expect("x86_64 marker branch"),
        };
        let persist = output.find("__rt_str_persist").expect("borrowed persistence path");
        let concat = output.rfind("_concat_off").expect("concat restore");
        assert!(branch < persist && persist < concat, "{name}: {output}");
    }
}

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
