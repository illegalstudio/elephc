//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::decref_any::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};


#[cfg(test)]
mod packed_integer_tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    /// Removes only this test's assembly and objects on success or assertion failure.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        /// Cleans the isolated cross-assembly directory.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    /// Integer leaves retain normal ownership even while graph nodes suppress back-edge releases.
    #[test]
    fn packed_integer_leaf_sweep_release_all_targets() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
            .join(format!("integer-leaf-release-{}-{nonce}", std::process::id())));
        fs::create_dir_all(&dir.0).unwrap();
        let clang = Command::new("clang").arg("--version").output().is_ok();
        for (name, triple) in [
            ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13.0"),
            ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
        ] {
            for pic in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.pic_data_refs = pic;
                emit_decref_any(&mut emitter);
                let assembly = emitter.output();
                let x86 = name == "linux-x86_64";
                let mask = if x86 { "and r11d, 0x7f" } else { "ubfx x14, x11, #8, #7" };
                let (_, gate) = assembly.split_once(mask).expect("integer storage gate");
                let graph = gate.find("__rt_decref_any_graph_kind:\n").unwrap();
                for dispatch in if x86 {
                    ["cmp r11d, 4", "jb __rt_decref_any_dispatch", "cmp r11d, 7", "ja __rt_decref_any_dispatch"]
                } else {
                    ["cmp x14, #4", "b.lo __rt_decref_any_dispatch", "cmp x14, #7", "b.hi __rt_decref_any_dispatch"]
                } {
                    assert!(gate.find(dispatch).is_some_and(|position| position < graph),
                        "{name}, PIC={pic}: non-graph arrays must bypass suppression: {dispatch}");
                }
                assert!(assembly.contains(if x86 { "test r10, 0x10000" } else { "tst x11, x14" }),
                    "{name}: ordinary graph-child reachability remains required");
                if clang {
                    let path = dir.0.join(format!("{name}-{pic}.s"));
                    let prefix = if x86 { ".intel_syntax noprefix\n" } else { "" };
                    fs::write(&path, format!(".text\n{prefix}{assembly}")).unwrap();
                    let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&path)
                        .arg("-o").arg(path.with_extension("o")).output().unwrap();
                    assert!(result.status.success(), "{name}, PIC={pic}: {}",
                        String::from_utf8_lossy(&result.stderr));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies every supported architecture routes throwable heap kind 6 through
    /// object release, including ARM64's active-cycle collection range.
    #[test]
    fn test_decref_any_dispatches_throwable_kind_on_all_supported_architectures() {
        for platform in [Platform::MacOS, Platform::Linux] {
            let mut emitter = Emitter::new(Target::new(platform, Arch::AArch64));
            emit_decref_any(&mut emitter);
            let assembly = emitter.output();
            assert!(
                assembly.contains("    cmp x13, #6\n"),
                "ARM64 collector range excludes throwables on {platform:?}:\n{assembly}"
            );
            assert!(
                assembly.contains("    cmp x11, #6\n    b.eq __rt_decref_any_object\n"),
                "ARM64 dispatcher excludes throwables on {platform:?}:\n{assembly}"
            );
        }

        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_decref_any(&mut emitter);
        let assembly = emitter.output();
        assert!(
            assembly.contains("    cmp r10, 6\n    je __rt_decref_any_object\n"),
            "x86_64 dispatcher excludes throwables:\n{assembly}"
        );
    }

    /// String dispatch releases one owner and reaches heap reclamation only at refcount zero.
    #[test]
    fn test_decref_any_string_dispatch_is_refcount_aware_on_all_supported_architectures() {
        for platform in [Platform::MacOS, Platform::Linux] {
            let mut emitter = Emitter::new(Target::new(platform, Arch::AArch64));
            emit_decref_any(&mut emitter);
            let assembly = emitter.output();
            assert!(
                assembly.contains(
                    "__rt_decref_any_string:\n    ldr w11, [x0, #-12]\n    subs w11, w11, #1\n    str w11, [x0, #-12]\n    b.ne __rt_decref_any_done\n    b __rt_heap_free\n"
                ),
                "ARM64 string release bypasses its refcount on {platform:?}:\n{assembly}"
            );
        }

        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_decref_any(&mut emitter);
        let assembly = emitter.output();
        assert!(
            assembly.contains(
                "__rt_decref_any_string:\n    mov r10d, DWORD PTR [rax - 12]\n    sub r10d, 1\n    mov DWORD PTR [rax - 12], r10d\n    jne __rt_decref_any_done\n    jmp __rt_heap_free\n"
            ),
            "x86_64 string release bypasses its refcount:\n{assembly}"
        );
    }
}
