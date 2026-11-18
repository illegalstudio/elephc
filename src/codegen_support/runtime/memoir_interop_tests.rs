//! Purpose:
//! Preserves string and interoperability storage assembly across the Memoir boundary.
//!
//! Called from:
//! - Focused compiler tests for string and interoperability extraction.
//!
//! Key details:
//! - Includes the APM fixed-data slots from main 8fd1043dbe.
//! - Captured before extraction at 9f41af9a4941cd37fa5e26265adc24a917bd9b68.
//! - Fingerprints include output bytes, fixed data and sorted internal-label bookkeeping.
//! - Every supported target covers executable, PIC, library, extension and dead-strip modes.

use super::{data, io, pointers, strings, zval};
use crate::codegen_support::{emit::Emitter, platform::Target};
use crate::php_version::PhpVersion;
use sha2::{Digest, Sha256};

/// Emits string and interoperability storage through the compiler entry points.
fn emit_interop(target: Target, mode: u8, capacity: usize) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emitter.concat_capacity = capacity;
    strings::emit_concat(&mut emitter);
    pointers::emit_str_to_cstr(&mut emitter);
    pointers::emit_cstr_to_str(&mut emitter);
    zval::emit_zval_string_new(&mut emitter);
    zval::emit_zval_free(&mut emitter);
    zval::emit_zval_free_array(&mut emitter);
    io::emit_pr_append(&mut emitter);
    io::emit_pr_write(&mut emitter);
    io::emit_pr_finish(&mut emitter);
    emitter
}

/// Preserves string/interoperability storage and fixed data captured before extraction.
#[test]
fn memoir_interop_preserve_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for capacity in [65_536, 134_217_728] {
                let mut emitter = emit_interop(Target::parse(name).unwrap(), mode, capacity);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let fixed = data::emit_runtime_data_fixed(capacity, Target::parse(name).unwrap(), PhpVersion::default());
                let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}\n{fixed}", emitter.output())));
                actual.push(format!("{name} {mode} {capacity} {hash}"));
            }
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_interop_baseline.txt").trim(),
        "string/interoperability helpers or fixed data changed during extraction");
}

/// Cross-assembles storage helpers and compiler routing on every target, mode and capacity.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_interop_assemble_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique sources and objects on every exit path.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-interop-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for capacity in [65_536, 134_217_728] {
                let emitter = emit_interop(Target::parse(name).unwrap(), mode, capacity);
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let source = dir.0.join(format!("{name}-{mode}-{capacity}.s"));
                let fixed = data::emit_runtime_data_fixed(capacity, Target::parse(name).unwrap(), PhpVersion::default());
                fs::write(&source, format!("{syntax}.text\n{}\n{fixed}", emitter.output())).unwrap();
                let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(result.status.success(), "{name}, mode={mode}, capacity={capacity}: {}",
                    String::from_utf8_lossy(&result.stderr));
            }
        }
    }
}
