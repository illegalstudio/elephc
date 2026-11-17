//! Purpose:
//! Pins foreign_lifetimes integration assembly before its Memoir extraction.
//!
//! Called from:
//! - Focused compiler tests for external owner lifetimes.
//!
//! Key details:
//! - Captured from the original compiler implementation at 13e62f2589.
//! - Output and sorted internal labels cover all five targets and six emission modes.
//! - Compiler-owned fixed data is outside this fingerprint.

use super::{owner_frames, release_boundary};
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Emits the original selected integration inventory with independent output flags.
fn emit_foreign_lifetimes(target: Target, mode: u8, variant: u8) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    match variant {
        0 => owner_frames::emit(&mut emitter),
        1 => release_boundary::emit(&mut emitter),
        2 => release_boundary::emit_reference_pin_release(&mut emitter),
        _ => unreachable!(),
    }
    emitter
}

/// Fingerprints one integration family and its sorted internal-label bookkeeping.
fn foreign_lifetimes_fingerprints(variants: &[u8]) -> String {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for &variant in variants {
                let mut emitter = emit_foreign_lifetimes(Target::parse(name).unwrap(), mode, variant);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
                actual.push(format!("{name} {mode} {variant} {hash}"));
            }
        }
    }
    actual.join("\n")
}

/// Cross-assembles every integration variant on all supported targets and output modes.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_foreign_lifetimes_assembles_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique source and object directory on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-foreign_lifetimes-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for variant in 0..3 {
                let emitter = emit_foreign_lifetimes(Target::parse(name).unwrap(), mode, variant);
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let source = dir.0.join(format!("{name}-{mode}-{variant}.s"));
                fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
                let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(output.status.success(), "{name}, mode={mode}: {}",
                    String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// Preserves external record publication, unlink and GC slot adaptation.
#[test]
fn memoir_external_owner_frames_preserve_assembly() {
    assert_eq!(foreign_lifetimes_fingerprints(&[0]), include_str!("memoir_external_frames_baseline.txt").trim());
}

/// Preserves both ordinary release and pin transfer through the boxed exception accumulator.
#[test]
fn memoir_foreign_release_preserves_assembly() {
    assert_eq!(foreign_lifetimes_fingerprints(&[1, 2]), include_str!("memoir_foreign_release_baseline.txt").trim());
}
