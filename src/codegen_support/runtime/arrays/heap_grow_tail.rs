//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::heap_grow_tail::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Owns one unique assembly-test directory and removes it after the test.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        /// Removes only this test's uniquely named directory.
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Cross-assembles direct and PIC tail-growth helpers for every supported target.
    #[test]
    fn heap_tail_growth_assembles_on_all_supported_targets() {
        if Command::new("clang").arg("--version").output().is_err() {
            eprintln!("Clang unavailable; executable target CI provides assembly coverage");
            return;
        }

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the system clock must be later than the Unix epoch")
            .as_nanos();
        let dir = Scratch(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!("tail-grow-assembly-{}-{nonce}", std::process::id())),
        );
        fs::create_dir_all(&dir.0).expect("the assembly scratch directory must be creatable");

        let targets = [
            ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13.0"),
            ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"),
            ("linux-x86_64", "x86_64-linux-gnu"),
        ];
        for (name, triple) in targets {
            for pic in [false, true] {
                let target = Target::parse(name)
                    .unwrap_or_else(|_| panic!("supported target {name} must parse"));
                let mut emitter = if pic {
                    Emitter::new_cdylib(target)
                } else {
                    Emitter::new(target)
                };
                if target.arch == Arch::X86_64 {
                    emitter.raw(".intel_syntax noprefix");
                }
                emitter.raw(".text");
                emit_heap_grow_tail(&mut emitter);
                let assembly = emitter.output();

                assert!(assembly.contains("_heap_off"), "{name}, pic={pic}");
                assert!(assembly.contains("_heap_limit"), "{name}, pic={pic}");
                assert!(assembly.contains("_gc_live"), "{name}, pic={pic}");
                assert!(assembly.contains("_gc_peak"), "{name}, pic={pic}");
                assert!(!assembly.contains("_gc_allocs"), "{name}, pic={pic}");
                assert!(!assembly.contains("_gc_frees"), "{name}, pic={pic}");
                if target.arch == Arch::X86_64 {
                    assert!(assembly.contains("mov DWORD PTR [rdi - 16], eax"));
                } else {
                    assert!(assembly.contains("str w1, [x0, #-16]"));
                }

                let source = dir.0.join(format!("{name}-{pic}.s"));
                fs::write(&source, assembly).expect("generated assembly must be writable");
                let output = Command::new("clang")
                    .args(["-target", triple, "-c"])
                    .arg(&source)
                    .arg("-o")
                    .arg(source.with_extension("o"))
                    .output()
                    .expect("Clang must run after its availability probe succeeds");
                assert!(
                    output.status.success(),
                    "{name}, pic={pic}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
