//! Purpose:
//! Cross-assembles native mbstring result materialization and both graph construction contracts.
//!
//! Called from:
//! - Focused compiler library tests with the explicit clang target gate enabled.
//!
//! Key details:
//! - All five supported targets are checked with direct and PIC data references.
//! - Runtime-GC tests separately execute scalar, hash, and indexed INI results on the host.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};
use std::process::Command;

/// Assembles complete mbstring adapters so INI branches and their common epilogues resolve together.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn mbstring_ini_materialize_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-ini-materialize-targets-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        for pic in [false, true] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.pic_data_refs = pic;
            if emitter.target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
            emitter.raw(".text");
            super::super::emit_mbstring(&mut emitter, false, false);
            let source = directory.join(format!("{name}-{pic}.s"));
            let object = directory.join(format!("{name}-{pic}.o"));
            std::fs::write(&source, emitter.output()).unwrap();
            let built = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(object).output().unwrap();
            assert!(built.status.success(), "{name}, PIC {pic}: {}", String::from_utf8_lossy(&built.stderr));
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
