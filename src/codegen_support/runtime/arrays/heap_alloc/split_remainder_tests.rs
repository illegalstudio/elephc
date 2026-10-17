//! Purpose:
//! Cross-assembles allocator split placement on every supported target.
//!
//! Called from:
//! - Focused allocator emitter unit tests and target CI jobs.
//!
//! Key details:
//! - Native ownership and list behavior are exercised by the runtime GC harness.
//! - Generated assembler includes the complete allocator, including its split branches.

use super::*;
use crate::codegen_support::platform::Target;
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Owns one unique, automatically removed workspace build directory.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Removes only the directory created by this test.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Checks target syntax and relocations without requiring target SDK libraries.
#[test]
fn small_split_remainder_assembles_on_all_supported_targets() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("Clang unavailable; executable target CI provides assembly coverage");
        return;
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("split-assembly-{}-{nonce}", std::process::id())));
    fs::create_dir_all(&dir.0).unwrap();
    for (name, triple) in [("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13.0"), ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu")]
    {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit_heap_alloc(&mut emitter);
        let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
        let source = dir.0.join(format!("{name}.s"));
        fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
        let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
            .arg("-o").arg(source.with_extension("o")).output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
