//! Purpose:
//! Pins weak-cache and hash-entry reference assembly before their Memoir extraction.
//!
//! Called from:
//! - Focused compiler tests for weak-cache and hash-entry reference extraction.
//!
//! Key details:
//! - Captured from compiler adapters before extraction on main 23314d3e1f.
//! - Output and sorted internal labels cover all five targets and six emission modes.
//! - Compiler-owned fixed data is outside this fingerprint.

use super::hash_entry_reference;
use super::super::objects;
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Emits the full weak-cache or hash-reference integration and two inline register layouts.
fn emit_references(target: Target, mode: u8, weak: bool) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    if weak {
        objects::emit_weak_reference(&mut emitter);
    } else {
        hash_entry_reference::emit_hash_entry_reference(&mut emitter);
        let registers = if target.arch == crate::codegen_support::platform::Arch::X86_64 {
            [("r9", "rcx", "r8"), ("r11", "rax", "rdx")]
        } else {
            [("x5", "x3", "x4"), ("x9", "x10", "x11")]
        };
        for (index, (tag, lo, hi)) in registers.into_iter().enumerate() {
            emitter.label_global(&format!("__memoir_inline_reference_probe_{index}"));
            hash_entry_reference::emit_inline_entry_deref(&mut emitter,
                &format!("__memoir_inline_reference_done_{index}"), tag, lo, hi);
            crate::codegen_support::abi::emit_return(&mut emitter);
        }
    }
    emitter
}

/// Fingerprints each complete emitter result and its dead-strip label bookkeeping.
fn reference_fingerprints(weak: bool) -> String {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            let mut emitter = emit_references(Target::parse(name).unwrap(), mode, weak);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
            actual.push(format!("{name} {mode} {hash}"));
        }
    }
    actual.join("\n")
}

/// Preserves wrapper cache publication, reads, retirement and label metadata.
#[test]
fn memoir_weak_cache_preserves_assembly() {
    assert_eq!(reference_fingerprints(true), include_str!("memoir_weak_cache_baseline.txt").trim(),
        "weak cache extraction changed assembly");
}

/// Preserves promotion, borrowed reads, inline register layouts and iterator integration.
#[test]
fn memoir_hash_references_preserve_assembly() {
    assert_eq!(reference_fingerprints(false), include_str!("memoir_hash_references_baseline.txt").trim(),
        "hash reference extraction changed assembly");
}

/// Cross-assembles both complete reference inventories on all targets and modes.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_reference_helpers_assemble_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique source and object directory on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-reference-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for weak in [false, true] {
                let emitter = emit_references(Target::parse(name).unwrap(), mode, weak);
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let source = dir.0.join(format!("{name}-{mode}-{weak}.s"));
                fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
                let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(output.status.success(), "{name}, mode={mode}: {}",
                    String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}
