//! Purpose:
//! Tests capture initialization against independent native ownership and destructor observations.
//!
//! Called from:
//! - Focused compiler library tests on supported Linux and macOS hosts.
//!
//! Key details:
//! - Hash allocation and Mixed boxing use the actual target emitters with a checked C allocator.
//! - C callbacks model pending exceptions; PHP unwinding requires separate integration coverage.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform, Target}, runtime::arrays};
use std::process::Command;

/// Checks publication order, reentrant replacement, deferred ownership, aliases, and malformed inputs.
#[test]
fn native_mbstring_capture_reference_begin_preserves_publication_and_owners() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-capture-begin-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    super::emit(&mut emitter);
    arrays::emit_hash_new(&mut emitter);
    arrays::emit_mixed_from_value(&mut emitter);
    adapters(&mut emitter);
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("capture.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("fixture.c"), include_str!("native_begin.c")).unwrap();
    let executable = directory.join("fixture");
    let built = Command::new("cc").current_dir(&directory)
        .args(["-O2", "-Wall", "-Wextra", "-Werror", "fixture.c", "capture.s", "-o"])
        .arg(&executable).output().unwrap();
    assert!(built.status.success(), "capture initialization fixture build failed: {}", String::from_utf8_lossy(&built.stderr));
    let result = Command::new(&executable).output().unwrap();
    let _ = std::fs::remove_dir_all(directory);
    assert!(result.status.success(), "capture initialization fixture failed: {}", String::from_utf8_lossy(&result.stderr));
}

/// Bridges private unary conventions to checked C ownership and rejects unused boxing paths.
fn adapters(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    for (symbol, destination) in [("__rt_heap_alloc", "fixture_allocate"),
        ("__rt_decref_hash", "fixture_release"), ("__rt_decref_any", "fixture_release"),
        ("__rt_incref", "fixture_retain")] {
        emitter.label_global(symbol);
        if !arm { emitter.instruction("mov rdi, rax"); }                        // adapt the private unary argument to C
        emitter.instruction(&format!("{} {destination}", if arm { "b" } else { "jmp" }));// transfer control to checked ownership accounting
    }
    emitter.label_global("fixture_invoke");
    if arm {
        emitter.instruction("mov x9, x0");                                      // retain the protected operation pointer
        emitter.instruction("mov x0, x1");                                      // pass its unary value argument
        emitter.instruction("br x9");                                           // delegate without another frame
    } else {
        emitter.instruction("mov r11, rdi");                                    // retain the protected operation pointer
        emitter.instruction("mov rax, rsi");                                    // pass the private unary input
        emitter.instruction("jmp r11");                                         // delegate without another frame
    }
    for symbol in ["__rt_str_persist", "__rt_resource_id_of"] {
        emitter.label_global(symbol);
        emitter.instruction(&format!("{} fixture_unsupported", if arm { "b" } else { "jmp" }));// reject unexpected string or resource boxing in this fixture
    }
}
