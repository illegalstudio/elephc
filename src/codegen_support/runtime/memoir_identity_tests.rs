//! Purpose:
//! Preserves identity and temporary-owner assembly across the Memoir boundary.
//!
//! Called from:
//! - Focused compiler tests for identity extraction.
//!
//! Key details:
//! - Includes the APM data slots and typed-property reference emitter from main 8fd1043dbe.
//! - Captured before extraction at 58ee974dc47d68a8984b096c860a7fc6465434e9.
//! - The optional mbstring emitter excludes the standalone UTF-8 validator after main f84cc7fae7.
//! - Fingerprints include output bytes, fixed data and sorted internal-label bookkeeping.
//! - Every supported target covers executable, PIC, library, extension and dead-strip modes.

use super::{arrays, data, exceptions, fibers, objects, reference_cells, strings};
use crate::codegen_support::{emit::Emitter, platform::Target};
use crate::php_version::PhpVersion;
use sha2::{Digest, Sha256};

/// Emits the moved identity primitives through their compiler entry points.
fn emit_identity(target: Target, mode: u8) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    arrays::emit_mixed_unbox(&mut emitter);
    exceptions::emit_cleanup_call_operand_owner(&mut emitter);
    exceptions::emit_throwable_boxed_owners(&mut emitter);
    reference_cells::emit_reference_cells(&mut emitter, true);
    strings::emit_mbstring(&mut emitter, true, false);
    objects::emit_object_handles(&mut emitter);
    fibers::emit_fiber_alloc_stack(&mut emitter);
    fibers::emit_fiber_free_stack(&mut emitter);
    emitter
}

/// Preserves identity primitives and fixed data captured before extraction.
#[test]
fn memoir_identity_preserve_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            let mut emitter = emit_identity(Target::parse(name).unwrap(), mode);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let fixed = data::emit_runtime_data_fixed(262_144, Target::parse(name).unwrap(), PhpVersion::default());
            let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}\n{fixed}", emitter.output())));
            actual.push(format!("{name} {mode} {hash}"));
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_identity_baseline.txt").trim(),
        "identity helpers or fixed runtime data changed during extraction");
}

/// Cross-assembles identity helpers for every target and emission mode.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_identity_assemble_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique sources and objects on every exit path.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-identity-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            let emitter = emit_identity(Target::parse(name).unwrap(), mode);
            let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
            let source = dir.0.join(format!("{name}-{mode}.s"));
            let fixed = data::emit_runtime_data_fixed(262_144, Target::parse(name).unwrap(), PhpVersion::default());
            fs::write(&source, format!("{syntax}.text\n{}\n{fixed}", emitter.output())).unwrap();
            let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(source.with_extension("o")).output().unwrap();
            assert!(result.status.success(), "{name}, mode={mode}: {}",
                String::from_utf8_lossy(&result.stderr));
        }
    }
}
