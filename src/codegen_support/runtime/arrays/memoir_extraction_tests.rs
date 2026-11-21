//! Purpose:
//! Pins memory-helper assembly while implementation moves into the compiler-side Memoir crate.
//!
//! Called from:
//! - Focused compiler unit tests during memory subsystem extraction.
//!
//! Key details:
//! - Fingerprints include symbol bookkeeping as well as assembly text.
//! - Every supported target covers executable, PIC, recoverable-library and hosted modes.

use super::*;
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Preserves target assembly and local-label identities across the Memoir extraction.
#[test]
fn memoir_extraction_preserves_memory_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            let mut emitter = emit_memory_helpers(Target::parse(name).unwrap(), mode);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
            actual.push(format!("{name} {mode} {hash}"));
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_extraction_baseline.txt").trim(),
        "allocator or collector assembly changed");
}

/// Emits the exact original allocator/collector inventory for fingerprints and target assembly.
fn emit_memory_helpers(target: Target, mode: usize) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emit_heap_alloc(&mut emitter);
    emit_heap_grow_tail(&mut emitter);
    emit_heap_consolidate(&mut emitter);
    emit_heap_free(&mut emitter, true, true);
    emit_heap_debug_fail(&mut emitter);
    emit_heap_debug_check_live(&mut emitter);
    emit_heap_debug_validate_free_list(&mut emitter);
    emit_heap_debug_report(&mut emitter);
    emit_heap_kind(&mut emitter);
    incref::emit_incref(&mut emitter);
    decref_array::emit_decref_array(&mut emitter);
    decref_hash::emit_decref_hash(&mut emitter);
    decref_object::emit_decref_object(&mut emitter);
    emit_decref_mixed(&mut emitter);
    emit_decref_any(&mut emitter);
    emit_gc_collect_cycles(&mut emitter);
    emit_gc_mark_reachable(&mut emitter);
    emit_gc_note_child_ref(&mut emitter);
    emit_gc_destructors(&mut emitter);
    emit_gc_control(&mut emitter);
    emit_memory(&mut emitter);
    gc_candidates::emit(&mut emitter);
    emitter
}

/// Cross-assembles allocator and collector helpers for every target and mode.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_core_assembles_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique sources and objects on every exit path.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-core-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            let emitter = emit_memory_helpers(Target::parse(name).unwrap(), mode);
            let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
            let source = dir.0.join(format!("{name}-{mode}.s"));
            fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
            let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(source.with_extension("o")).output().unwrap();
            assert!(result.status.success(), "{name}, mode={mode}: {}",
                String::from_utf8_lossy(&result.stderr));
        }
    }
}
