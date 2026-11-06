//! Purpose:
//! Checks integer-token range/bounds and its integration with recursive JSON ownership.
//!
//! Called from:
//! - The focused library test harness through `super`.
//!
//! Key details:
//! - Direct/PIC assembly is checked for every supported target when Clang is available.
//! - A Linux x86_64 guard-page probe executes the emitted leaf at the exact input boundary.

use super::*;
use crate::codegen_support::platform::Target;
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Removes only the assembly, objects and probe owned by one test invocation.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Cleans the owned directory even when an assertion fails.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Creates an isolated directory below this worktree's target directory.
fn scratch() -> Scratch {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("json-integer-{}-{nonce}", std::process::id())));
    fs::create_dir_all(&dir.0).unwrap();
    dir
}

/// The leaf never allocates or changes flags, error coordinates or recursive depth.
#[test]
fn json_integer_prefix_probe_assembles_all_targets() {
    let clang = Command::new("clang").arg("--version").output().is_ok();
    let dir = scratch();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13.0"),
        ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for pic in [false, true] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emitter.pic_data_refs = pic;
            integers::emit(&mut emitter);
            let assembly = emitter.output();
            assert!(!assembly.contains("    bl ") && !assembly.contains("    call "), "{name}");
            assert!(!assembly.contains("_json_error") && !assembly.contains("_json_active"), "{name}");
            if clang {
                let mut parser = Emitter::new(target);
                parser.pic_data_refs = pic;
                if name == "linux-x86_64" { arrays::emit_x86_64(&mut parser); }
                else { arrays::emit_aarch64(&mut parser); }
                let source = dir.0.join(format!("{name}-{pic}.s"));
                let prefix = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                fs::write(&source, format!(".text\n{prefix}{assembly}{}", parser.output())).unwrap();
                let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(output.status.success(), "{name}, PIC={pic}: {}",
                    String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// Prefix failure widens once, while success rejoins existing close/error cleanup paths.
#[test]
fn json_integer_prefix_frames_and_conversion_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for pic in [false, true] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.pic_data_refs = pic;
            emit_json_decode_mixed(&mut emitter);
            let assembly = emitter.output();
            let x86 = name == "linux-x86_64";
            let call = if x86 { "    call " } else { "    bl " };
            let (_, array) = assembly.split_once("__rt_json_decode_mixed_array_real:\n").unwrap();
            let (prefix, _) = array.split_once(if x86 {
                "__rt_json_decode_array_real_scan_x:\n"
            } else { "__rt_json_decode_array_real_scan:\n" }).unwrap();
            assert!(prefix.find(&format!("{call}__rt_json_integer_token")).unwrap()
                < prefix.find(&format!("{call}__rt_array_to_mixed")).unwrap(), "{name}, PIC={pic}");
            assert_eq!(array.matches(&format!("{call}__rt_array_to_mixed\n")).count(), 1,
                "{name}, PIC={pic}");
            assert!(prefix.contains(&format!("{call}__rt_array_push_int\n")), "{name}");
            if x86 {
                assert!(prefix.contains("sub rsp, 64"));
                assert!(prefix.contains("[rbp - 56], rdi") && prefix.contains("[rbp - 64], rdx"));
            } else {
                assert!(prefix.contains("sub sp, sp, #80"));
                assert!(prefix.contains("stp x29, x30, [sp, #64]"));
                assert!(prefix.contains("str x1, [sp, #48]") && prefix.contains("str x2, [sp, #56]"));
            }
        }
    }
}

/// Real emitted x86 instructions handle extrema, strict grammar and exact guard-page bounds.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn json_integer_prefix_guard_page_and_random_i64_payloads() {
    if Command::new("clang").arg("--version").output().is_err() { return; }
    let dir = scratch();
    let mut emitter = Emitter::new(Target::parse("linux-x86_64").unwrap());
    integers::emit(&mut emitter);
    let assembly = dir.0.join("token.s");
    fs::write(&assembly, format!(".intel_syntax noprefix\n.text\n{}\n.section .note.GNU-stack,\"\",@progbits\n",
        emitter.output())).unwrap();
    let source = dir.0.join("probe.c");
    fs::write(&source, include_str!("integer_token_probe.c")).unwrap();
    let binary = dir.0.join("probe");
    let output = Command::new("clang").args(["-O2", "-mno-red-zone", "-o"]).arg(&binary)
        .arg(&source).arg(&assembly).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = Command::new(binary).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"integer token bounds and payloads: ok\n");
}
