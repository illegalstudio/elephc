//! Purpose:
//! Executes nested query selection with independent native ownership and destructor observations.
//!
//! Called from:
//! - Focused compiler library tests, with an explicit clang gate for foreign targets.
//!
//! Key details:
//! - Real hash lookup, pins, insertion, COW, guards, and deletion operate on a checked C allocator.
//! - C cleanup models pending status; runtime-GC codegen tests exercise the actual PHP unwinder.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform, Target}, runtime::{arrays, strings}};
use std::process::Command;

/// Verifies nested COW, Mixed wrappers, references, reentrant replacement, and pending cleanup.
#[test]
fn native_mbstring_query_enter_preserves_nested_ownership() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-query-enter-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    super::emit(&mut emitter);
    super::super::capture_hash::emit(&mut emitter);
    super::super::capture_destination::emit(&mut emitter);
    super::super::capture_reference::emit_store(&mut emitter);
    for emit in [arrays::emit_hash_new, arrays::emit_hash_grow, arrays::emit_hash_insert_owned,
        arrays::emit_hash_iter, arrays::emit_hash_get, arrays::emit_hash_key_hash,
        arrays::emit_hash_key_eq, arrays::emit_hash_fnv1a, arrays::emit_hash_pin,
        arrays::emit_hash_write_guards, arrays::emit_hash_set, arrays::emit_hash_unset,
        arrays::emit_hash_to_mixed, arrays::emit_array_to_hash, arrays::emit_mixed_from_value,
        arrays::emit_mixed_clone, arrays::emit_mixed_unbox, arrays::emit_mixed_reference,
        arrays::emit_hash_ensure_unique, arrays::emit_hash_clone_shallow, strings::emit_str_eq] {
        emit(&mut emitter);
    }
    super::super::capture_hash::tests::adapters(&mut emitter);
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("query.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("capture_fixture.c"), include_str!("../capture_hash/native_store.c")).unwrap();
    std::fs::write(directory.join("fixture.c"), include_str!("native_enter.c")).unwrap();
    let executable = directory.join("fixture");
    let built = Command::new("cc").current_dir(&directory)
        .args(["-O2", "-Wall", "-Wextra", "-Werror", "fixture.c", "query.s", "-o"])
        .arg(&executable).output().unwrap();
    assert!(built.status.success(), "query entry fixture build failed: {}", String::from_utf8_lossy(&built.stderr));
    let result = Command::new(&executable).output().unwrap();
    assert!(result.status.success(), "query entry fixture {} failed: {}", directory.display(), String::from_utf8_lossy(&result.stderr));
    std::fs::remove_dir_all(directory).unwrap();
}

/// Checks actual instruction encodings and relocations for every supported query-entry target.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn mbstring_query_enter_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-query-enter-targets-{}-{nonce}", std::process::id()));
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
        eprintln!("query entry assembled: {name}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
