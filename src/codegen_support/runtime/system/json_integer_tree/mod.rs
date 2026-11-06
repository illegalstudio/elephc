//! Purpose:
//! Emits the bounded fast path for JSON indexed-array trees containing only integers.
//! Keeps eligibility conservative while sharing one allocation and one linear write pass.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` for helper emission.
//! - The indexed-array JSON encoders before their general formatter state machines.
//!
//! Key details:
//! - Unsupported values return to the existing encoder without changing JSON or ownership state.
//! - Successful results use transient string ownership so ordinary JSON boxing adopts them once.

mod aarch64;
mod x86_64;

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the size probe, integer-width helper, writer, and allocating entry point.
pub(crate) fn emit_json_integer_tree(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64::emit_helpers(emitter),
        Arch::X86_64 => x86_64::emit_helpers(emitter),
    }
}

/// Emits a guarded attempt at the integer-tree encoder and falls through when unsupported.
pub(crate) fn emit_json_integer_tree_entry(emitter: &mut Emitter, prefix: &str) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64::emit_entry(emitter, prefix),
        Arch::X86_64 => x86_64::emit_entry(emitter, prefix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Owns and removes the scratch directory used by cross-assembly checks.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        /// Removes only the directory created by this test, including after assertion failure.
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Every supported target emits the bounded helper family and guarded entry sequence.
    #[test]
    fn integer_tree_helpers_cover_every_supported_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).expect("supported target");
            for pic in [false, true] {
                let mut emitter = if pic {
                    Emitter::new_cdylib(target)
                } else {
                    Emitter::new(target)
                };
                emit_json_integer_tree(&mut emitter);
                emit_json_integer_tree_entry(&mut emitter, "__test_json_int_tree");
                let asm = emitter.output();
                assert!(asm.contains("__rt_json_integer_tree_try:"), "{name} pic={pic}");
                assert!(asm.contains("__rt_json_integer_tree_probe:"), "{name} pic={pic}");
                assert!(asm.contains("__rt_json_integer_tree_write:"), "{name} pic={pic}");
                assert!(asm.contains("__rt_itoa_into"), "{name} pic={pic}");
                assert!(asm.contains("__rt_heap_alloc"), "{name} pic={pic}");
                assert!(asm.contains("_json_active_flags"), "{name} pic={pic}");
                assert!(asm.contains("_json_active_depth"), "{name} pic={pic}");
            }
        }
    }

    /// Cross-assembles the helper family for every supported target and relocation mode.
    #[test]
    fn integer_tree_helpers_assemble_on_every_supported_target() {
        if Command::new("clang").arg("--version").output().is_err() {
            eprintln!("Clang unavailable; target CI supplies assembly coverage");
            return;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let root = PathBuf::from(
            std::env::var_os("CARGO_TARGET_DIR")
                .unwrap_or_else(|| format!("{}/target", env!("CARGO_MANIFEST_DIR")).into()),
        );
        let scratch = Scratch(root.join(format!(
            "json-integer-tree-asm-{}-{nonce}",
            std::process::id()
        )));
        fs::create_dir_all(&scratch.0).expect("create assembly scratch directory");
        for (name, triple) in [
            ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13.0"),
            ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"),
            ("linux-x86_64", "x86_64-linux-gnu"),
        ] {
            for pic in [false, true] {
                let target = Target::parse(name).expect("supported target");
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_json_integer_tree(&mut emitter);
                emit_json_integer_tree_entry(&mut emitter, "__test_json_int_tree");
                let syntax = if target.arch == Arch::X86_64 {
                    ".intel_syntax noprefix\n"
                } else {
                    ""
                };
                let source = scratch.0.join(format!("{name}-{pic}.s"));
                fs::write(&source, format!("{syntax}.text\n{}", emitter.output()))
                    .expect("write emitted assembly");
                let output = Command::new("clang")
                    .args(["-target", triple, "-c"])
                    .arg(&source)
                    .arg("-o")
                    .arg(source.with_extension("o"))
                    .output()
                    .expect("run clang cross-assembler");
                assert!(
                    output.status.success(),
                    "{name} PIC={pic}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
