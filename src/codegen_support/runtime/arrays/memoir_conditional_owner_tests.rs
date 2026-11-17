//! Purpose:
//! Pins conditional_owner integration assembly before its Memoir extraction.
//!
//! Called from:
//! - Focused compiler tests for external owner lifetimes.
//!
//! Key details:
//! - Captured from the original compiler implementation at 13e62f2589.
//! - Output and sorted internal labels cover all five targets and six emission modes.
//! - Compiler-owned fixed data is outside this fingerprint.

use super::container_refusal_owner;
use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Emits the original selected integration inventory with independent output flags.
fn emit_conditional_owner(target: Target, mode: u8, variant: u8) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emitter.label_global("__memoir_conditional_owner_probe");
    abi::emit_frame_prologue(&mut emitter, 128);
    if variant == 0 { container_refusal_owner::emit_release(&mut emitter); }
    else { container_refusal_owner::emit_throw(&mut emitter); }
    abi::emit_frame_restore(&mut emitter, 128);
    abi::emit_return(&mut emitter);
    emitter
}

/// Fingerprints one integration family and its sorted internal-label bookkeeping.
fn conditional_owner_fingerprints(variants: &[u8]) -> String {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for &variant in variants {
                let mut emitter = emit_conditional_owner(Target::parse(name).unwrap(), mode, variant);
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
fn memoir_conditional_owner_assembles_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique source and object directory on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-conditional_owner-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for variant in 0..2 {
                let emitter = emit_conditional_owner(Target::parse(name).unwrap(), mode, variant);
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

/// Preserves conditional release and the full compiler refusal path around it.
#[test]
fn memoir_conditional_owner_preserves_assembly() {
    assert_eq!(conditional_owner_fingerprints(&[0, 1]), include_str!("memoir_conditional_owner_baseline.txt").trim());
}
