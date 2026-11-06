//! Purpose:
//! Cross-assembles JSON Unicode flag and enum handling on every supported target.
//!
//! Called from:
//! - Focused `cargo test --lib json_compatibility` runs and target CI.
//!
//! Key details:
//! - Executable PHP behavior is covered by the JSON codegen integration tests.
//! - Direct and PIC modes exercise shared runtime symbol access on both architectures.
//! - Scratch files are confined to one unique directory and removed on every exit path.

use super::*;
use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Owns only the scratch directory allocated by this test.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Removes the owned assembly and object files even after a failed assertion.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Checks the changed encoder branches and relocations for all five supported targets.
#[test]
fn json_compatibility_assembles_on_all_supported_targets() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("Clang unavailable; target CI provides assembly coverage");
        return;
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let scratch = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("json-assembly-{}-{nonce}", std::process::id())));
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
            emit_json_encode_str(&mut emitter);
            emit_json_encode_object(&mut emitter);
            let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let source = scratch.0.join(format!("{name}-{pic}.s"));
            fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
            let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(source.with_extension("o")).output().unwrap();
            assert!(output.status.success(), "{name} PIC={pic}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
