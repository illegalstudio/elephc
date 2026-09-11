//! Purpose:
//! Executes collector assembly against independent cyclic heap fixtures.
//!
//! Called from:
//! - Focused compiler library tests on Linux and macOS hosts.
//!
//! Key details:
//! - C provides explicit object, hash, Mixed, and reference ownership edges.
//! - Cleanup stubs check traversal order, pending state, and new allocation survival.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform, Target}, runtime::arrays};
use std::process::Command;

/// Checks real host instructions for rooted cycles, intact destructor reads, and resumed collection.
#[test]
fn native_gc_cycle_destructor_graph_and_cleanup_state() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-cycle-fixture-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    arrays::emit_gc_collect_cycles(&mut emitter);
    arrays::emit_gc_mark_reachable(&mut emitter);
    arrays::emit_gc_note_child_ref(&mut emitter);
    arrays::emit_gc_eval_object_children(&mut emitter);
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("collector.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("fixture.c"), include_str!("native_graph.c")).unwrap();
    let executable = directory.join("fixture");
    let built = Command::new("cc").current_dir(&directory)
        .args(["-Wall", "-Wextra", "-Werror", "fixture.c", "collector.s", "-o"])
        .arg(&executable).output().unwrap();
    assert!(built.status.success(), "collector fixture build failed: {}", String::from_utf8_lossy(&built.stderr));
    let result = Command::new(&executable).output().unwrap();
    let _ = std::fs::remove_dir_all(directory);
    assert!(result.status.success(), "collector fixture failed: {}", String::from_utf8_lossy(&result.stderr));
}
