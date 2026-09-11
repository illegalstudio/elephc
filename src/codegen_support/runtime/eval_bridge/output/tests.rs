//! Purpose:
//! Cross-assembles protected eval output requests for every supported target.
//!
//! Called from:
//! - The compiler library test harness with a multi-target clang assembler.
//!
//! Key details:
//! - C symbol mangling, the exception record, and six-argument starts use the actual emitter.

use super::*;
use crate::codegen_support::platform::Target;
use std::process::Command;

/// Assembles all output actions with the actual exception boundary on each supported target.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn protected_eval_output_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("eval-output-targets-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        emit(&mut emitter);
        let features = RuntimeFeatures::none();
        match target.arch {
            Arch::AArch64 => super::super::runtime_builtin_dispatch::emit_aarch64_runtime_builtin_dispatch(
                &mut emitter, features,
            ),
            Arch::X86_64 => super::super::runtime_builtin_dispatch::emit_x86_64_runtime_builtin_dispatch(
                &mut emitter, features,
            ),
        }
        let assembly = emitter.output();
        assert!(assembly.contains(&target.extern_symbol("__elephc_eval_ob_start_ex")));
        assert!(assembly.contains("__rt_eval_output_invalid:"));
        let source = directory.join(format!("{name}.s"));
        std::fs::write(&source, assembly).unwrap();
        let built = Command::new("clang").args(["-target", triple, "-c"]).arg(source)
            .arg("-o").arg(directory.join(format!("{name}.o"))).output().unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
    }
    std::fs::remove_dir_all(directory).unwrap();
}
