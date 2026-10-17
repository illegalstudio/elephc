//! Purpose:
//! Cross-assembles JSON token diagnostics and error-only decode validation for every target.
//!
//! Called from:
//! - The runtime JSON validator unit-test module.
//!
//! Key details:
//! - Direct and PIC relocation modes exercise both shared runtime architectures.
//! - The test removes its owned scratch directory on success and failure.

use super::*;
use crate::codegen_support::platform::{Arch, Target};
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Owns the scratch directory used for one cross-assembly check.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Retires only files created by this test, including after a failed assembler assertion.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Checks all changed diagnostics on five targets with direct and GOT-relative data access.
#[test]
fn json_coordinate_diagnostics_assemble_on_all_targets() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("Clang unavailable; target CI supplies assembly coverage");
        return;
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR")
        .unwrap_or_else(|| format!("{}/target", env!("CARGO_MANIFEST_DIR")).into()));
    let scratch = Scratch(root.join(format!("json-coordinates-asm-{}-{nonce}", std::process::id())));
    fs::create_dir_all(&scratch.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13.0"),
        ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"),
        ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for pic in [false, true] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emitter.pic_data_refs = pic;
            emit_json_validate(&mut emitter);
            super::super::json_throw_error::emit_json_throw_error(&mut emitter);
            let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let source = scratch.0.join(format!("{name}-{pic}.s"));
            fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
            let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(source.with_extension("o")).output().unwrap();
            assert!(output.status.success(), "{name} PIC={pic}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
