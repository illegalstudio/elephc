//! Purpose:
//! Verifies canonical string-null boxing and its assembly on every supported target.
//!
//! Called from:
//! - Focused runtime emitter tests before native strict string boundary checks.
//!
//! Key details:
//! - Borrowed and owned boxers classify the pointer before treating a miss as string data.
//! - Cross-assembly checks cover both relocation modes without linking native libraries.

use super::*;
use crate::codegen_support::platform::Target;
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Owns exactly the generated cross-assembly inputs and objects of this test.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Retires only this test's directory on success or assertion failure.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Missing string markers never reach persistence or an owned cell's string tag.
#[test]
fn string_null_boxers_assemble_on_all_supported_targets() {
    let clang = Command::new("clang").arg("--version").output().is_ok();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    fs::create_dir_all(&parent).unwrap();
    let path = parent.join(format!("string-null-boxers-{}-{stamp}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13.0"),
        ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for pic in [false, true] {
            let target = Target::parse(name).unwrap();
            let mut emitter = if pic { Emitter::new_pic(target) } else { Emitter::new(target) };
            emit_mixed_from_value(&mut emitter);
            emit_mixed_from_owned_string(&mut emitter);
            crate::codegen_support::runtime::arrays::emit_mixed_unbox(&mut emitter);
            let body = emitter.output();
            let string = body.split_once("__rt_mixed_from_value_string:\n").unwrap().1
                .split_once("__rt_mixed_from_value_retain:\n").unwrap().0;
            assert!(string.find("__rt_mixed_from_value_null_container").unwrap()
                < string.find("__rt_str_persist").unwrap(), "{name}: original null before persistence");
            let owned = body.split_once("__rt_mixed_from_owned_string_null:\n").unwrap().1
                .split_once("__rt_mixed_from_owned_string_done:\n").unwrap().0;
            assert!(owned.contains(if target.arch == Arch::AArch64 { "stp xzr, xzr, [x0, #8]" }
                else { "mov QWORD PTR [rax + 16], 0" }), "{name}: canonical null has no payload");
            if clang {
                let prefix = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
                let source = scratch.0.join(format!("{name}-{pic}.s"));
                fs::write(&source, format!("{prefix}.text\n{body}")).unwrap();
                let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(output.status.success(), "{name}, PIC={pic}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
    if !clang { eprintln!("Clang unavailable; target CI provides assembly validation"); }
}
