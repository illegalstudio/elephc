//! Purpose:
//! Executes stable hash growth, lifetime pins, append history, and COW with independent ownership accounting.
//!
//! Called from:
//! - Compiler library tests on supported Linux and macOS hosts.
//!
//! Key details:
//! - Real emitted insertion, lookup, iteration, and rehash code operate on C fixtures.
//! - An explicit clang check assembles the changed helpers for all supported targets.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform, Target}, runtime::arrays};
use std::process::Command;

/// Proves stable growth, persistent indices, append exhaustion cleanup, and logical COW ownership.
#[test]
fn native_hash_owned_growth_preserves_identity_and_entries() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-hash-growth-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    arrays::emit_hash_new(&mut emitter);
    arrays::emit_hash_grow(&mut emitter);
    arrays::emit_hash_ensure_unique(&mut emitter);
    arrays::emit_hash_clone_shallow(&mut emitter);
    arrays::emit_hash_pin(&mut emitter);
    arrays::emit_hash_insert_owned(&mut emitter);
    arrays::emit_hash_iter(&mut emitter);
    arrays::emit_hash_get(&mut emitter);
    arrays::emit_hash_key_hash(&mut emitter);
    arrays::emit_hash_key_eq(&mut emitter);
    arrays::emit_hash_fnv1a(&mut emitter);
    arrays::emit_hash_append(&mut emitter);
    arrays::emit_hash_set(&mut emitter);
    arrays::emit_hash_unset(&mut emitter);
    arrays::emit_hash_write_guards(&mut emitter);
    for (symbol, destination) in [
        ("__rt_heap_alloc", "fixture_allocate"), ("__rt_heap_free", "fixture_free"),
        ("__rt_decref_hash", "fixture_release"), ("__rt_decref_any", "fixture_release"),
        ("__rt_callable_descriptor_release", "fixture_release"),
    ] {
        emitter.label_global(symbol);
        if target.arch == Arch::X86_64 {
            emitter.instruction("mov rdi, rax");                                // adapt the private heap argument to the C ABI
            emitter.instruction(&format!("jmp {destination}"));                 // let the independent allocator own the return
        } else {
            emitter.instruction(&format!("b {destination}"));                   // pass the standard AArch64 argument unchanged
        }
    }
    emitter.label_global("__rt_object_handle_acquire");
    emitter.instruction("ret");                                                 // fixture Error objects need no external handle registry
    emitter.label_global("fixture_grow_owned");
    if target.arch == Arch::X86_64 {
        emitter.instruction("xor eax, eax");                                    // make the unrelated return register differ from the declared pointer argument
        emitter.instruction("jmp __rt_hash_grow_owned");                        // exercise the internal entry with only its declared C ABI input
    } else {
        emitter.instruction("b __rt_hash_grow_owned");                          // pass the declared AArch64 input to the internal entry
    }
    emitter.label_global("fixture_lookup");
    if target.arch == Arch::X86_64 {
        emitter.instruction("sub rsp, 8");                                      // align the nested runtime lookup
        emitter.instruction("mov rdx, -1");                                     // select integer-key lookup
        emitter.instruction("call __rt_hash_get");                              // fetch the actual runtime entry
        emitter.instruction("mov rax, r8");                                     // expose the entry address to the C assertion
        emitter.instruction("add rsp, 8");                                      // restore the caller stack
    } else {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve linkage across runtime lookup
        emitter.instruction("mov x2, #-1");                                     // select integer-key lookup
        emitter.instruction("bl __rt_hash_get");                                // fetch the actual runtime entry
        emitter.instruction("mov x0, x4");                                      // expose the entry address to the C assertion
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore caller linkage
    }
    emitter.instruction("ret");                                                 // return the borrowed matching entry
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("hash.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("fixture.c"), include_str!("native_growth.c")).unwrap();
    let executable = directory.join("fixture");
    let built = Command::new("cc").current_dir(&directory)
        .args(["-O2", "-Wall", "-Wextra", "-Werror", "fixture.c", "hash.s", "-o"])
        .arg(&executable).output().unwrap();
    assert!(built.status.success(), "hash fixture build failed: {}", String::from_utf8_lossy(&built.stderr));
    let result = Command::new(&executable).output().unwrap();
    let _ = std::fs::remove_dir_all(directory);
    assert!(result.status.success(), "hash fixture failed: {}", String::from_utf8_lossy(&result.stderr));
}

/// Assembles the changed hash allocation, insertion, append, and COW helpers for every supported target.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn hash_append_indices_assemble_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("hash-index-targets-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        if emitter.target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        for emit in [arrays::emit_hash_new, arrays::emit_hash_set, arrays::emit_hash_insert_owned,
            arrays::emit_hash_grow, arrays::emit_hash_clone_shallow, arrays::emit_hash_append,
            arrays::emit_hash_get, arrays::emit_hash_unset] {
            emit(&mut emitter);
        }
        crate::codegen_support::runtime::zval::emit_zval_pack_array_hash(&mut emitter);
        crate::codegen_support::runtime::zval::emit_zval_unpack_array(&mut emitter);
        crate::codegen_support::runtime::eval_bridge::array_next_index::emit(&mut emitter);
        let source = directory.join(format!("{name}.s"));
        let object = directory.join(format!("{name}.o"));
        std::fs::write(&source, emitter.output()).unwrap();
        let built = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
            .arg("-o").arg(object).output().unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
        eprintln!("hash indices assembled: {name}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
