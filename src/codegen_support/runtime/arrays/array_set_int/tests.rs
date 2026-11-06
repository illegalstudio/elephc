//! Purpose:
//! Cross-assembles the scalar indexed-array setter's unique overwrite and COW fallback paths.
//!
//! Called from:
//! - Focused runtime emitter unit tests and supported-target CI jobs.
//!
//! Key details:
//! - Direct and PIC emission must keep the in-bounds store before the shared-array slow path.
//! - The slow path must retain copy-on-write splitting for every supported target.

use super::*;
use crate::codegen_support::platform::Target;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// Owns one unique build directory and removes it after the test.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Removes only the directory created by this test.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Cross-assembles the fast and slow scalar setter paths on every supported target.
#[test]
fn scalar_array_set_paths_assemble_on_all_supported_targets() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("Clang unavailable; executable target CI provides assembly coverage");
        return;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = Scratch(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("array-set-int-assembly-{}-{nonce}", std::process::id())),
    );
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
            let mut emitter = if pic {
                Emitter::new_pic(target)
            } else {
                Emitter::new(target)
            };
            emit_array_set_int(&mut emitter);
            let assembly = emitter.output();
            let fast = assembly
                .find("overwrite a unique in-bounds scalar slot")
                .expect("unique overwrite fast path");
            let slow = assembly
                .find("__rt_array_set_int_slow:")
                .expect("shared-array slow path");
            let cow = assembly[slow..]
                .find("__rt_array_ensure_unique")
                .map(|offset| slow + offset)
                .expect("copy-on-write helper call");
            let store = match target.arch {
                Arch::AArch64 => assembly.find("str x2, [x9, x1, lsl #3]").unwrap(),
                Arch::X86_64 => assembly
                    .find("mov QWORD PTR [rdi + 24 + rsi * 8], rdx")
                    .unwrap(),
            };
            assert!(fast < store && store < slow && slow < cow, "{name}, PIC={pic}");

            let syntax = if target.arch == Arch::X86_64 {
                ".intel_syntax noprefix\n"
            } else {
                ""
            };
            let source = dir.0.join(format!("{name}-{pic}.s"));
            fs::write(&source, format!("{syntax}.text\n{assembly}")).unwrap();
            let output = Command::new("clang")
                .args(["-target", triple, "-c"])
                .arg(&source)
                .arg("-o")
                .arg(source.with_extension("o"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{name}, PIC={pic}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
