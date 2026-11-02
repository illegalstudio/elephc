//! Purpose:
//! Cross-assembles the guarded heap-free path for every supported target and relocation mode.
//!
//! Called from:
//! - Focused allocator emitter unit tests and the target CI matrix.
//!
//! Key details:
//! - Runtime GC tests exercise scan counts, coalescing, tail reclamation and ownership.
//! - Both direct and PIC data references must preserve the endpoint comparison registers.

use super::*;
use crate::codegen_support::platform::Target;
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Owns only the unique directory created for this cross-assembly test.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Removes generated assembly and objects on success or assertion failure.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Checks the real free helper's instructions and relocations in ten target/mode combinations.
#[test]
fn heap_tail_guard_assembles_on_all_supported_targets() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("Clang unavailable; executable target CI provides assembly coverage");
        return;
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("heap-tail-assembly-{}-{nonce}", std::process::id())));
    fs::create_dir_all(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13.0"),
        ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"),
        ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for pic in [false, true] {
            let target = Target::parse(name).unwrap();
            let mut emitter = if pic { Emitter::new_pic(target) } else { Emitter::new(target) };
            emit_heap_free(&mut emitter, false, false);
            let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let source = dir.0.join(format!("{name}-{pic}.s"));
            fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
            let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(source.with_extension("o")).output().unwrap();
            assert!(output.status.success(), "{name}, PIC {pic}: {}",
                String::from_utf8_lossy(&output.stderr));
        }
    }
}
