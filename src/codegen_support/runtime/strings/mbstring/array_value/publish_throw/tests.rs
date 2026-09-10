//! Purpose:
//! Verifies the emitted Throwable-publication action through independent native C ownership counters.
//!
//! Called from:
//! - The compiler's focused mbstring runtime unit tests on supported Unix hosts.
//!
//! Key details:
//! - Executes actual host assembly and the shared Mixed unboxer.
//! - C counters distinguish raw-object retention, boxed-owner consumption, and failure precedence.

use super::*;
use std::process::Command;
use crate::codegen_support::{platform::{Platform, Target}, runtime::arrays};

/// Adapts native incref arguments to the independent C counter without simulating the publication code.
fn emit_incref_fixture(emitter: &mut Emitter) {
    emitter.label_global("__rt_incref");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // pass the native raw-object argument to the independent SysV counter
    }
    let jump = if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" };
    let symbol = emitter.target.extern_symbol("mb_test_incref");
    emitter.instruction(&format!("{jump} {symbol}"));                           // return the C counter directly to the original native caller
}

/// Confirms ownership is published before input release and every returned failure preserves consumption.
#[test]
fn mbstring_native_throw_publication_ownership() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-mbstring-publish-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    emit(&mut emitter);
    arrays::emit_mixed_unbox(&mut emitter);
    emit_incref_fixture(&mut emitter);
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("publication.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("publication.c"), include_str!("native_publication.c")).unwrap();
    let binary = directory.join("publication");
    let output = Command::new("cc").current_dir(&directory)
        .args(["-fPIC", "-Wall", "-Wextra", "-Werror", "publication.c", "publication.s", "-o"])
        .arg(&binary).output().unwrap();
    assert!(output.status.success(), "publication build failed: {}", String::from_utf8_lossy(&output.stderr));
    let output = Command::new(&binary).output().unwrap();
    assert!(output.status.success(), "publication checks failed: {}", String::from_utf8_lossy(&output.stderr));
    std::fs::remove_dir_all(directory).unwrap();
}
