//! Purpose:
//! Executes capture stores with independent native ownership and destructor observations.
//!
//! Called from:
//! - Focused compiler library tests on supported Linux and macOS hosts.
//!
//! Key details:
//! - Real hash growth, lookup, insertion, COW, pins, conversion, and guarded mutations use a C allocator.
//! - C cleanup models pending throws; the fixture does not claim PHP exception-unwinder coverage.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform, Target}, runtime::{arrays, strings}};
use std::process::Command;

/// Checks binary captures, aliases, reentrant mutations, retargeting, and complete ownership release.
#[test]
fn native_mbstring_capture_hash_store_preserves_aliases_and_pending_cleanup() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-capture-hash-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    super::emit(&mut emitter);
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
    adapters(&mut emitter);
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("capture.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("fixture.c"), include_str!("native_store.c")).unwrap();
    let executable = directory.join("fixture");
    let built = Command::new("cc").current_dir(&directory)
        .args(["-O2", "-Wall", "-Wextra", "-Werror", "fixture.c", "capture.s", "-o"])
        .arg(&executable).output().unwrap();
    assert!(built.status.success(), "capture fixture build failed: {}", String::from_utf8_lossy(&built.stderr));
    let result = Command::new(&executable).output().unwrap();
    let _ = std::fs::remove_dir_all(directory);
    assert!(result.status.success(), "capture fixture failed: {}", String::from_utf8_lossy(&result.stderr));
}

/// Adapts private runtime conventions to independent C allocation and release operations.
pub(in crate::codegen_support::runtime::strings::mbstring) fn adapters(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_heap_kind");
    emitter.instruction(if arm { "ldr x0, [x0, #-8]" } else { "mov rax, QWORD PTR [rax - 8]" }); // read the independently allocated fixture's uniform heap-kind word
    emitter.instruction(if arm { "and x0, x0, #255" } else { "and eax, 255" }); // expose the fixture kind without the production arena range check
    emitter.instruction("ret");                                                 // return the independently stamped heap kind
    for (symbol, destination) in [("__rt_heap_alloc", "fixture_allocate"), ("__rt_heap_free", "fixture_free"),
        ("__rt_decref_hash", "fixture_release"), ("__rt_decref_any", "fixture_release"),
        ("__rt_callable_descriptor_release", "fixture_release"), ("__rt_incref", "fixture_retain")] {
        emitter.label_global(symbol);
        if !arm { emitter.instruction("mov rdi, rax"); }                        // adapt the private runtime unary input to C
        emitter.instruction(&format!("{} {destination}", if arm { "b" } else { "jmp" })); // delegate ownership accounting to the C fixture
    }
    emitter.label_global("fixture_invoke");
    if arm {
        emitter.instruction("mov x9, x0");                                      // retain the protected operation pointer
        emitter.instruction("mov x0, x1");                                      // supply the unary value argument
        emitter.instruction("br x9");                                           // delegate without another native frame
    } else {
        emitter.instruction("mov r11, rdi");                                    // retain the protected operation pointer
        emitter.instruction("mov rax, rsi");                                    // supply the private unary value convention
        emitter.instruction("mov rdi, rsi");                                    // also supply C input for hash unpin
        emitter.instruction("jmp r11");                                         // delegate without another native frame
    }
    emitter.label_global("fixture_lookup");
    emitter.instruction(if arm { "stp x29, x30, [sp, #-16]!" } else { "sub rsp, 8" }); // retain linkage and align nested lookup
    emitter.instruction(if arm { "bl __rt_hash_get" } else { "call __rt_hash_get" }); // borrow the actual matching entry
    emitter.instruction(if arm { "mov x0, x4" } else { "mov rax, r8" });        // expose the entry address to C observations
    emitter.instruction(if arm { "ldp x29, x30, [sp], #16" } else { "add rsp, 8" }); // restore caller linkage and stack
    emitter.instruction("ret");                                                 // return the borrowed entry or a miss
    persist_adapter(emitter);
}

/// Preserves the private string pointer/length return convention across C byte allocation.
fn persist_adapter(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_str_persist");
    if arm {
        emitter.instruction("sub sp, sp, #32");                                 // reserve length and aligned linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // retain the caller across allocation
        emitter.instruction("str x2, [sp]");                                    // retain the exact binary byte length
        emitter.instruction("mov x0, x1");                                      // pass the borrowed byte pointer through C
        emitter.instruction("mov x1, x2");                                      // pass the exact length through C
        emitter.instruction("bl fixture_persist");                              // acquire an independently tracked string owner
        emitter.instruction("mov x1, x0");                                      // return the native string pointer
        emitter.instruction("ldr x2, [sp]");                                    // return the exact byte length
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore caller linkage
        emitter.instruction("add sp, sp, #32");                                 // release adapter storage
    } else {
        emitter.instruction("sub rsp, 24");                                     // align the C call and reserve the byte length
        emitter.instruction("mov QWORD PTR [rsp], rdx");                        // retain the exact binary byte length
        emitter.instruction("mov rdi, rax");                                    // pass borrowed bytes through C
        emitter.instruction("mov rsi, rdx");                                    // pass the exact length through C
        emitter.instruction("call fixture_persist");                            // acquire an independently tracked string owner
        emitter.instruction("mov rdx, QWORD PTR [rsp]");                        // return length beside the native pointer
        emitter.instruction("add rsp, 24");                                     // release adapter storage
    }
    emitter.instruction("ret");                                                 // transfer the string owner to native construction
}
