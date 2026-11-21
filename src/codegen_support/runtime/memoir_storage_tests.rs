//! Purpose:
//! Preserves growable storage and ownership-guard assembly across the Memoir boundary.
//!
//! Called from:
//! - Focused compiler tests for storage emitter extraction.
//!
//! Key details:
//! - Fingerprints include output bytes and sorted internal-label bookkeeping.
//! - Every supported target covers library/PIC modes and both mbstring metadata variants.

use super::{arrays, callables, strings};
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Emits all four selected storage blocks through the compiler's actual target emitter.
fn emit_storage(target: Target, mode: u8, mbstring: bool) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emitter.concat_capacity = strings::concat_scratch::scratch_capacity(
        if mbstring { 134_217_728 } else { 262_144 });
    strings::emit_strbuf(&mut emitter);
    strings::emit_str_append(&mut emitter, mbstring);
    callables::emit_callable_descriptor_release(&mut emitter);
    arrays::emit_hash_write_guards(&mut emitter);
    emitter
}

/// Pins growable storage, callable cleanup and hash guard output before extraction.
#[test]
fn memoir_storage_preserves_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for mbstring in [false, true] {
                let mut emitter = emit_storage(Target::parse(name).unwrap(), mode, mbstring);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
                actual.push(format!("{name} {mode} {mbstring} {hash}"));
            }
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_storage_baseline.txt").trim(),
        "storage helpers or internal labels changed during extraction");
}

/// Cross-assembles the frozen storage inventory for all supported targets and modes.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_storage_assembles_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique sources and objects on every exit path.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-storage-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for mbstring in [false, true] {
                let emitter = emit_storage(Target::parse(name).unwrap(), mode, mbstring);
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let source = dir.0.join(format!("{name}-{mode}-{mbstring}.s"));
                fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
                let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(result.status.success(), "{name}, mode={mode}, mbstring={mbstring}: {}",
                    String::from_utf8_lossy(&result.stderr));
            }
        }
    }
}
