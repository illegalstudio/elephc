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
        "97f9af5a10afa4e0ec39da5087f2d440049fcf97a5812dcd11c0e7b964eccf29",
        "e3623cc0eadbc4869452940686a016358513102bf5be6ec9082f82e4f2e61332",
        "a23d00d332b9f82c08d85c912b2f725d1f178d886aea535298a6459a219f377c",
        "d01b42e6f32bdb738fecf99dc82461dc2d9f20655dd46134d01c7def1bf2f750",
        "4b8b29426f9caa8be47ff01d3361ca3c48aef1421c874080cbf09ec31efdc6fa",
        "67a441c9f8b165cd8a71a8322b58cb950d250ad8285e3e6a5b23ec01b890a106",
    ],
    [
        "2c8c157aaa15232257b73fd0b790ad7340787d525412992ae8eb98155aaa0a9e",
        "7d7653db0fca15cbba9f4298443748ec5e9d974d5465df76fd472e95c3d90de7",
        "157d12bd3714fbadddfd5ea3c98cbf3f7c259301b820b733864552c029aad22b",
        "c8ceacb4666d8ee46fff2828acae7b20721aee38a77a070694cc1f187243af8b",
        "f9a12a7247d5158136d08a41c5b40e535e7b9955cd55d5a6ac18322087e25e0f",
        "2c8c157aaa15232257b73fd0b790ad7340787d525412992ae8eb98155aaa0a9e",
    ],
    [
        "a31262a2d09beb13af7ae30cf711749dc392b2620d082837f7f6a8988760c53d",
        "eca8e035231e566ab7965cab71b5b2fb071fefda86439a0fa647c6815e39fedf",
        "226f989a3bd3e6c47f21af1d4e917414c8b7e049359a9c9601ffb70c704057bd",
        "8decdd914a1db25aff59800d178d1d5322e3a4416d773d8b5bd135a4fa19e00e",
        "a84936921539d2e3af289530d39b4101b7d17e469b98f04ccde38289e050a0ed",
        "a31262a2d09beb13af7ae30cf711749dc392b2620d082837f7f6a8988760c53d",
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
