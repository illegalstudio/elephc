//! Purpose:
//! Pins inline Mixed symbol publication before its Memoir extraction.
//!
//! Called from:
//! - Focused compiler tests for static Mixed publication extraction.
//!
//! Key details:
//! - Captured from the original compiler implementation at f768abafa9.
//! - Output and sorted labels cover five targets, six modes, two release policies and three owner slots.
//! - Compiler-owned fixed data is outside this fingerprint.

use super::publish_mixed_symbol;
use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Configures one independent compiler emitter without sharing mutable label or output state.
fn configured_emitter(target: Target, mode: u8) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emitter
}

/// Emits the original publication protocol inside a frame containing all selected owner slots.
fn emit_mixed_symbol(target: Target, mode: u8, release_previous: bool, transfer_owner: Option<usize>) -> Emitter {
    let mut emitter = configured_emitter(target, mode);
    emitter.label_global("__memoir_mixed_symbol_probe");
    abi::emit_frame_prologue(&mut emitter, 8192);
    publish_mixed_symbol(&mut emitter, "_memoir_mixed_symbol_fixture",
        release_previous, transfer_owner, "__memoir_mixed_symbol_store");
    abi::emit_frame_restore(&mut emitter, 8192);
    abi::emit_return(&mut emitter);
    emitter
}

/// Fingerprints each complete emitter result and its dead-strip label bookkeeping.
fn mixed_symbol_fingerprints() -> String {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for release_previous in [false, true] {
                for transfer_owner in [None, Some(32), Some(4096)] {
                    let mut emitter = emit_mixed_symbol(Target::parse(name).unwrap(), mode, release_previous, transfer_owner);
                    let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                    labels.sort();
                    let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
                    actual.push(format!("{name} {mode} {release_previous} {transfer_owner:?} {hash}"));
                }
            }
        }
    }
    actual.join("\n")
}

/// Preserves publication order, slot transfer, prior-owner policy and internal-label bookkeeping.
#[test]
fn memoir_mixed_symbol_preserves_assembly() {
    assert_eq!(mixed_symbol_fingerprints(), include_str!("memoir_mixed_symbol_baseline.txt").trim(),
        "Mixed symbol publication changed during extraction");
}

/// Uses host layout and owner policy while retiring transferred storage before reentrant release.
#[test]
fn memoir_mixed_symbol_uses_host_layout_and_owner_policy() {
    use elephc_memoir::{reference_cells::mixed_symbol::{self, Context}, FrameOffset, RecordOffset};

    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for release_previous in [false, true] {
                for transfer_owner in [None, Some(32), Some(4096)] {
                    let target = Target::parse(name).unwrap();
                    let mut emitter = configured_emitter(target, mode);
                    let mut marker = configured_emitter(target, mode);
                    let mut transfer = configured_emitter(target, mode);
                    let context = Context {
                        symbol: "_memoir_host_mixed_symbol",
                        initialization_offset: RecordOffset::new(24),
                        release_previous,
                        transfer_owner: transfer_owner.map(FrameOffset::new),
                        prefix: "__memoir_host_mixed_prefix",
                    };
                    mixed_symbol::publish(&mut emitter, context);
                    let output = emitter.output();
                    assert!(output.contains("__memoir_host_mixed_prefix_payload:"), "{name}, mode={mode}");
                    assert!(output.contains(context.symbol), "{name}, mode={mode}");
                    abi::emit_store_zero_to_symbol(&mut marker, context.symbol, context.initialization_offset.bytes());
                    let initialized = output.find(&marker.output()).expect("host marker store missing");
                    if let Some(offset) = transfer_owner {
                        abi::emit_store_zero_to_local_slot(&mut transfer, offset);
                        let transferred = output.find(&transfer.output()).expect("transferred owner clear missing");
                        assert!(transferred < initialized, "{name}, mode={mode}: parking owner survived publication");
                    }
                    if release_previous {
                        let release = output.find("__rt_decref_any").unwrap();
                        assert!(initialized < release, "{name}, mode={mode}: destructor preceded initialization");
                    } else {
                        assert!(!output.contains("__rt_decref_any"), "{name}, mode={mode}: implicit previous-owner release");
                    }
                }
            }
        }
    }
}

/// Cross-assembles every publication variant on all supported targets and output modes.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_mixed_symbol_assembles_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique source and object directory on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-mixed_symbol-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for release_previous in [false, true] {
                for transfer_owner in [None, Some(32), Some(4096)] {
                    let emitter = emit_mixed_symbol(Target::parse(name).unwrap(), mode, release_previous, transfer_owner);
                    let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                    let source = dir.0.join(format!("{name}-{mode}-{release_previous}-{transfer_owner:?}.s"));
                    fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
                    let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                        .arg("-o").arg(source.with_extension("o")).output().unwrap();
                    assert!(output.status.success(), "{name}, mode={mode}: {}",
                        String::from_utf8_lossy(&output.stderr));
                }
            }
        }
    }
}
