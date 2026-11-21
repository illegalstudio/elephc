//! Purpose:
//! Preserves Mixed storage and cleanup proofs assembly across the Memoir boundary.
//!
//! Called from:
//! - Focused compiler tests for Mixed storage extraction.
//!
//! Key details:
//! - Includes the APM data slots and typed-property reference emitter from main 8fd1043dbe.
//! - Captured before extraction at 775d2eb9bb5c5bdc2c6b0d244796323067705137.
//! - Fingerprints include output bytes, fixed data and sorted internal-label bookkeeping.
//! - Every supported target covers executable, PIC, library, extension and dead-strip modes.

use super::{arrays, data, exceptions, reference_cells};
use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};
use crate::php_version::PhpVersion;
use sha2::{Digest, Sha256};

/// Emits Mixed storage and cleanup proofs through the compiler entry points.
fn emit_mixed_storage(target: Target, mode: u8, native_validation: bool) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    arrays::emit_array_unbox_slots(&mut emitter);
    reference_cells::emit_reference_cells(&mut emitter, native_validation);
    exceptions::emit_cleanup_invoke(&mut emitter);
    emitter.label_global("__memoir_mixed_reference_slot_probe");
    let (address, value, marker) = if target.arch == Arch::AArch64 {
        ("x9", "x10", "x12")
    } else { ("r11", "r10", "r8") };
    reference_cells::follow_mixed_reference_slot(&mut emitter, address, value, marker,
        "__memoir_mixed_reference_slot_done");
    crate::codegen_support::abi::emit_return(&mut emitter);
    emitter
}

/// Preserves Mixed storage, cleanup proofs and fixed data captured before extraction.
#[test]
fn memoir_mixed_storage_preserve_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for native_validation in [false, true] {
                let mut emitter = emit_mixed_storage(Target::parse(name).unwrap(), mode, native_validation);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let fixed = data::emit_runtime_data_fixed(262_144, Target::parse(name).unwrap(), PhpVersion::default());
                let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}\n{fixed}", emitter.output())));
                actual.push(format!("{name} {mode} {native_validation} {hash}"));
            }
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_mixed_storage_baseline.txt").trim(),
        "Mixed storage, cleanup proofs or fixed data changed during extraction");
}

/// Cross-assembles Mixed storage and cleanup gates on every target, mode and validation setting.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_mixed_storage_assemble_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique sources and objects on every exit path.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-mixed-storage-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for native_validation in [false, true] {
                let emitter = emit_mixed_storage(Target::parse(name).unwrap(), mode, native_validation);
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let source = dir.0.join(format!("{name}-{mode}-{native_validation}.s"));
                let fixed = data::emit_runtime_data_fixed(262_144, Target::parse(name).unwrap(), PhpVersion::default());
                fs::write(&source, format!("{syntax}.text\n{}\n{fixed}", emitter.output())).unwrap();
                let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(result.status.success(), "{name}, mode={mode}, native_validation={native_validation}: {}",
                    String::from_utf8_lossy(&result.stderr));
            }
        }
    }
}
