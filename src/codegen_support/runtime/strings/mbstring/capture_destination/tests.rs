//! Purpose:
//! Assembles capture invocation, reference dispatch, and indexed conversion on every target.
//!
//! Called from:
//! - Focused compiler library verification with the ignored clang-dependent test enabled.
//!
//! Key details:
//! - This checks real instruction encoding and relocations, including both iOS variants.
//! - Native executable integration tests separately validate values and ownership.

use super::*;
use crate::codegen_support::{platform::Target, runtime::arrays};
use std::process::Command;

/// Assembles the complete changed helper bodies for all five supported targets.
#[test]
#[ignore = "requires clang with ELF AArch64/x86_64 and Apple AArch64 assembler support"]
fn mbstring_capture_destination_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-capture-targets-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"),
        ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        super::emit(&mut emitter);
        super::super::capture_reference::emit(&mut emitter);
        super::super::capture_invoke::emit(&mut emitter, true);
        super::super::deferred_capture::emit(&mut emitter);
        super::super::catalog::emit(&mut emitter);
        super::super::native::emit(&mut emitter, true);
        arrays::emit_array_to_hash(&mut emitter);
        let source = directory.join(format!("{name}.s"));
        let object = directory.join(format!("{name}.o"));
        std::fs::write(&source, emitter.output()).unwrap();
        let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
            .arg("-o").arg(&object).output().expect("clang is required for this explicit target check");
        assert!(output.status.success(), "{name}: {}\n{}", source.display(), String::from_utf8_lossy(&output.stderr));
        eprintln!("capture destination assembled: {name}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
