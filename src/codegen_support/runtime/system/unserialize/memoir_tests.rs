//! Purpose:
//! Pins complete unserialize assembly before moving its owner primitives into Memoir.
//!
//! Called from:
//! - Focused compiler tests for hydration ownership extraction.
//!
//! Key details:
//! - Captured from the original compiler implementation at b1a089c827.
//! - Output and sorted internal labels cover all five targets and six emission modes.
//! - Compiler-owned fixed data is outside this fingerprint.

use super::emit_unserialize;
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Emits the original complete parser inventory with independently selected output flags.
fn emit_hydration(target: Target, mode: u8) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emit_unserialize(&mut emitter);
    emitter
}

/// Fingerprints each complete emitter result and its dead-strip label bookkeeping.
fn hydration_fingerprints() -> String {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            let mut emitter = emit_hydration(Target::parse(name).unwrap(), mode);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
            actual.push(format!("{name} {mode} {hash}"));
        }
    }
    actual.join("\n")
}

/// Preserves parser integration, ownership helpers and label metadata across extraction.
#[test]
fn memoir_hydration_preserves_assembly() {
    assert_eq!(hydration_fingerprints(), include_str!("memoir_hydration_baseline.txt").trim(),
        "hydration ownership extraction changed complete unserialize assembly");
}

/// Cross-assembles the complete parser and owner inventory on all targets and modes.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_hydration_assembles_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique source and object directory on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-hydration-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            let emitter = emit_hydration(Target::parse(name).unwrap(), mode);
            let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
            let source = dir.0.join(format!("{name}-{mode}.s"));
            fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
            let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(source.with_extension("o")).output().unwrap();
            assert!(output.status.success(), "{name}, mode={mode}: {}",
                String::from_utf8_lossy(&output.stderr));
        }
    }
}
