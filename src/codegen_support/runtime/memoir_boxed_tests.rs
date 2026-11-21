//! Purpose:
//! Preserves boxed-value and protected-reference assembly across the Memoir boundary.
//!
//! Called from:
//! - Focused compiler tests for boxed-value extraction.
//!
//! Key details:
//! - Fingerprints include output bytes and sorted internal-label bookkeeping.
//! - Every supported target covers executable, PIC, library, extension and dead-strip modes.

use super::{arrays, exceptions};
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Emits Mixed construction, persistent references and protected owner retirement.
fn emit_boxed(target: Target, mode: u8) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    arrays::emit_mixed_from_value(&mut emitter);
    arrays::emit_mixed_from_owned_string(&mut emitter);
    arrays::emit_mixed_reference(&mut emitter);
    exceptions::emit_local_ref_cell_release(&mut emitter);
    exceptions::emit_cleanup_preserve_exception(&mut emitter);
    emitter
}

/// Preserves boxed-value and protected-release output captured before extraction.
#[test]
fn memoir_boxed_values_preserve_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            let mut emitter = emit_boxed(Target::parse(name).unwrap(), mode);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
            actual.push(format!("{name} {mode} {hash}"));
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_boxed_baseline.txt").trim(),
        "boxed-value or protected-release assembly changed during extraction");
}

/// Cross-assembles boxed constructors and cleanup helpers for every target and mode.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_boxed_values_assemble_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique sources and objects on every exit path.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-boxed-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            let emitter = emit_boxed(Target::parse(name).unwrap(), mode);
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
