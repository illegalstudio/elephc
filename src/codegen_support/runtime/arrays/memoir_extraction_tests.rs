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

// Captured before extraction from main 67760076f89c9d0eaaa0863b0fe25e3ce67d5b75.
const BASELINE: [[&str; 6]; 3] = [
    [
        "c741cbff75bfb12e4810c4a654fd2636131ebd7a31e043c4f99b62bc34eff5be",
        "d5017ccc0aa1ee5042214a9897b639f04d77a86cb7a884febbd08424cf6dd231",
        "1d273b68346c615b0536c9bd952f442e11432aba4a36b309312bdcd12720ad4d",
        "1e1dc87c029f774e0776b749c61deaedd8b73291ad466b5172b18b2bf1aa3608",
        "8730987ab5a56f2d9efb50ab193a174d8939185ca1d64aa19face5ff3e3b4d11",
        "1a5b22f53c133cf86eca3bb40994e48c1687c9d511069e89bfd682fa0d957db3",
    ],
    [
        "369e99a9eb961ac4a3dd34c55468d98d8d3b100b2201b536dfa813005a542755",
        "5ab05bfc2f73bf19420e92f869d8f0fcea152435db2aaf282733858c411662b8",
        "ad243f9629aed665d996c93c8d4b87109997bc54c1d653e2d41f9d427ffe414e",
        "9f1e5e7c8a3c9822c7b9caf74b62ca6e1f29a2016d1799b74f11c4d9e8b052b0",
        "a8c57d6efcbba29bf9e07476909a11f29ce41ae8115224e63cca15fdc2c6ebd4",
        "369e99a9eb961ac4a3dd34c55468d98d8d3b100b2201b536dfa813005a542755",
    ],
    [
        "a692bb9a334156939d4951777a814436019e4a988517f46120e84ba07de29a79",
        "e5aef177e99cfc1ecc651acb2ad1cc54c2adb10b1c3e9da6b9a6a43dab1be1c1",
        "d8dd10ff754dfc73a6e505ad624854eb12cc4acec009c7a3fbdf0b9affa31b3a",
        "fafc4d4ccc215207edb0e5a9988425aa6c5ac983395e1f10dd96ae55e38c7a4b",
        "94028c1dc7129fdcb8443fa048f6c5dff6b126b9966422d3854917d53a67ebb9",
        "a692bb9a334156939d4951777a814436019e4a988517f46120e84ba07de29a79",
    ],
];

/// Preserves target assembly and local-label identities across the Memoir extraction.
#[test]
fn memoir_extraction_preserves_memory_assembly() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            let mut emitter = emit_memory_helpers(Target::parse(name).unwrap(), mode);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
            let platform = match name { "linux-aarch64" => 1, "linux-x86_64" => 2, _ => 0 };
            assert_eq!(hash, BASELINE[platform][mode], "{name}, mode {mode}");
        }
    }
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
