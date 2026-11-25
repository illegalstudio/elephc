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
        "7cc2eca24ac4b724456caa5437da11366e7f8197f345b7cd149c2702edd8fb14",
        "0a6e5fe5010d9ee755193ce795592014dfb3b6f1396d8cc9a9c6e4162f058b21",
        "3db9e1994ffee84db38dea673ebfb4608898073fc9c7ea1a9ac80eb267fb34ee",
        "a8dc214dc70f6de6a76830f8a2d770a92533b1bfb2acdb27c1eb702118bb4dd0",
        "3c322a57a4c749aa147b68a32ddee3f250696ef59227e6cf27f948e1ea4349b8",
        "b0492087c1f2d82b67eb82f4e9ca543a80da3ecc77324c0ed86bfe4e4c8ecd9a",
    ],
    [
        "bd080e24fb323638050c1e282563fb4556e95c93bc8c81101368da7b87931f55",
        "18153b03b5bad990c584054eaab878146451e2b9b63ce8ab8976517d6a80327d",
        "f85388bd0774fce95d26c1d225cb2f14be98734d2a779c7f61bac7a40f6890c2",
        "6b3311ec8772235ffd4ec00e40d3371e38311ed099979f82db43c99a8bbe5b91",
        "900fc659440e29dfe4e9126d44e84c1a8b335dce8d0532c873e3c1dcd1ee723f",
        "bd080e24fb323638050c1e282563fb4556e95c93bc8c81101368da7b87931f55",
    ],
    [
        "f80e4ec2ddee8185c9f41c8b91596d7ad028946e91e4ba40b94f3d1153e7d580",
        "8f751a0fb7e28c561434a6aac4945104a5463fce21fd50ca29114d17097bbc6b",
        "496866e8437ead84f42a0680126eb67aaf65757943a0464bc2f44f1c61703c6e",
        "32c0924d6a60e168652dc26183d885f3362a8a381d5aaf716ad000743933d756",
        "11f4f3e0dda232aeb745f3248bdb581a70ad0b2d0a5e7c71c642e519404d8ac7",
        "f80e4ec2ddee8185c9f41c8b91596d7ad028946e91e4ba40b94f3d1153e7d580",
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
