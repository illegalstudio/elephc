//! Purpose:
//! Validates target encodings for native query registration and its storage callbacks.
//!
//! Called from:
//! - Focused compiler library tests with an explicit cross-target clang gate.
//!
//! Key details:
//! - Runtime-GC tests separately execute the adapter against the actual Rust bridge and PHP runtime.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform, Target}};
use std::process::Command;

/// Covers every query-storage callback plus the seven-argument registration adapter.
#[test]
fn windows_query_storage_callbacks_use_c_abi_entries() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    super::emit(&mut emitter);
    let assembly = emitter.output();
    for callback in ["__rt_mbstring_query_root", "__rt_mbstring_query_next",
        "__rt_mbstring_query_hash_enter", "__rt_mbstring_capture_hash_store",
        "__rt_mbstring_query_hash_remove", "__rt_mbstring_query_release"] {
        let name = format!("{callback}_cabi");
        let body = assembly.split(&format!("{name}:\n")).nth(1).expect("C ABI callback");
        let entry = body.split(&format!("{name}__internal:")).next().unwrap();
        assert!(entry.contains("mov rdi, rcx"), "{name}: missing MSx64 adapter");
    }
    let register = assembly.split("__rt_mbstring_query_register:\n").nth(1)
        .expect("query register C ABI entry");
    let register = register.split("__rt_mbstring_query_register__internal:\n").next()
        .expect("query register adapter before its internal implementation");
    assert!(register.contains("mov r10, QWORD PTR [rsp + 56]"));
    assert!(register.contains("mov r8, QWORD PTR [rsp + 40]"));
    assert!(register.contains("mov r9, QWORD PTR [rsp + 48]"));
    assert!(register.contains("mov QWORD PTR [rsp], r10"));
    assert!(!register.contains("QWORD PTR [rsp + 64]"));
    assert!(register.contains("movdqu XMMWORD PTR [rsp + 24], xmm6"));
    assert!(register.contains("movdqu xmm15, XMMWORD PTR [rsp + 168]"));
    assert!(register.contains("mov rdi, QWORD PTR [rsp + 8]"));
    assert!(register.contains("mov rsi, QWORD PTR [rsp + 16]"));
    assert!(assembly.contains("lea r11, [rip + elephc_mbstring_query_apply_v1]"));
    assert!(assembly.contains("mov QWORD PTR [rsp + 48], rax"));
    assert!(assembly.contains("mov QWORD PTR [rsp + 56], r10"));
    assert!(!assembly.contains("call elephc_mbstring_query_apply_v1"));
}

/// The first reference lookup must retain both linkage and the writable output pointer.
#[test]
fn mbstring_query_root_preserves_aarch64_entry_state() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        super::storage::emit(&mut emitter);
        let assembly = emitter.output();
        let root = assembly.find("__rt_mbstring_query_root:").unwrap();
        let lookup = assembly[root..].find("bl __rt_mbstring_reference_child_slot").unwrap() + root;
        let save_linkage = assembly[root..].find("stp x29, x30, [sp, #16]").unwrap() + root;
        let save_output = assembly[root..].find("str x2, [sp]").unwrap() + root;
        assert!(save_linkage < lookup && save_output < lookup, "{name}");
        assert!(assembly[lookup..].contains("cbz x0, __rt_mbstring_query_root_failed"), "{name}");
    }
}

/// Assembles the C7-to-C8 adapter and storage callbacks for every supported target.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn mbstring_query_register_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-query-register-targets-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        if emitter.target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        super::emit(&mut emitter);
        let source = directory.join(format!("{name}.s"));
        let object = directory.join(format!("{name}.o"));
        std::fs::write(&source, emitter.output()).unwrap();
        let built = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
            .arg("-o").arg(object).output().unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
        eprintln!("query registration assembled: {name}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
