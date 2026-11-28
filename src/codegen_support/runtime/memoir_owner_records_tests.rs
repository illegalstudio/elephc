//! Purpose:
//! Preserves static binding and owner-record assembly across the Memoir boundary.
//!
//! Called from:
//! - Focused compiler tests for owner-record extraction.
//!
//! Key details:
//! - Includes the APM data slots and typed-property reference emitter from main 8fd1043dbe.
//! - Captured before extraction at 2b9042bfa4ccc51d5c03bef28101641d7e3e5186.
//! - Fingerprints include output bytes, fixed data and sorted internal-label bookkeeping.
//! - Every supported target covers executable, PIC, library, extension and dead-strip modes.

use super::{data, exceptions, reference_cells, system};
use crate::codegen_support::{emit::Emitter, platform::Target};
use crate::php_version::PhpVersion;
use elephc_memoir::data::DataTarget;
use sha2::{Digest, Sha256};

/// Emits static bindings and owner records through compiler entry points.
fn emit_owner_records(target: Target, mode: u8, native_validation: bool) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    reference_cells::emit_reference_cells(&mut emitter, native_validation);
    exceptions::emit_exception_cleanup_frames(&mut emitter);
    exceptions::emit_cleanup_invoke(&mut emitter);
    system::emit_owner_records_for_memoir_test(&mut emitter);
    emitter.label_global("__memoir_static_binding_read_probe");
    reference_cells::load_mixed_static_value(&mut emitter, "__memoir_static_binding_fixture", native_validation);
    crate::codegen_support::abi::emit_return(&mut emitter);
    emitter
}

/// Preserves owner-record storage and fixed data captured before extraction.
#[test]
fn memoir_owner_records_preserve_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for native_validation in [false, true] {
                let mut emitter = emit_owner_records(Target::parse(name).unwrap(), mode, native_validation);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let target = Target::parse(name).unwrap();
                let mut fixed = data::emit_runtime_data_fixed(
                    262_144,
                    target,
                    PhpVersion::default(),
                    crate::codegen_support::RuntimeFeatures::none(),
                );
                fixed.push_str(&target.common("__memoir_static_binding_fixture", reference_cells::STATIC_BINDING_BYTES));
                let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}\n{fixed}", emitter.output())));
                actual.push(format!("{name} {mode} {native_validation} {hash}"));
            }
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_owner_records_baseline.txt").trim(),
        "static bindings, owner records or fixed data changed during extraction");
}

/// Cross-assembles owner records on every target, mode and native-validation setting.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_owner_records_assemble_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique sources and objects on every exit path.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-owner-records-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for native_validation in [false, true] {
                let emitter = emit_owner_records(Target::parse(name).unwrap(), mode, native_validation);
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let source = dir.0.join(format!("{name}-{mode}-{native_validation}.s"));
                let target = Target::parse(name).unwrap();
                let mut fixed = data::emit_runtime_data_fixed(
                    262_144,
                    target,
                    PhpVersion::default(),
                    crate::codegen_support::RuntimeFeatures::none(),
                );
                fixed.push_str(&target.common("__memoir_static_binding_fixture", reference_cells::STATIC_BINDING_BYTES));
                fs::write(&source, format!("{syntax}.text\n{}\n{fixed}", emitter.output())).unwrap();
                let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(result.status.success(), "{name}, mode={mode}, native_validation={native_validation}: {}",
                    String::from_utf8_lossy(&result.stderr));
            }
        }
    }
}
