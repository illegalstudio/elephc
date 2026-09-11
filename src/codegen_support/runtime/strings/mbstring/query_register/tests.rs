//! Purpose:
//! Validates target encodings for native query registration and its storage callbacks.
//!
//! Called from:
//! - Focused compiler library tests with an explicit cross-target clang gate.
//!
//! Key details:
//! - Runtime-GC tests separately execute the adapter against the actual Rust bridge and PHP runtime.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};
use std::process::Command;

/// Assembles the C7-to-C8 adapter and storage callbacks for every supported target.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn mbstring_query_register_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-query-register-targets-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        if emitter.target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        super::emit(&mut emitter);
        let source = directory.join(format!("{name}.s"));
        let object = directory.join(format!("{name}.o"));
        std::fs::write(&source, emitter.output()).unwrap();
        let built = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
            .arg("-o").arg(object).output().unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
        eprintln!("query registration assembled: {name}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
