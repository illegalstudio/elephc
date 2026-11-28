//! Purpose:
//! Preserves foreign-owner integration assembly across its Memoir extraction.
//!
//! Called from:
//! - Focused compiler tests for external owner lifetimes.
//!
//! Key details:
//! - Captured from the original compiler implementation at 13e62f2589.
//! - Output and sorted internal labels cover all five targets and six emission modes.
//! - Compiler-owned fixed data is outside this fingerprint.

use super::{owner_frames, release_boundary};
use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Creates independent output state for one supported target and emission mode.
fn configured_emitter(target: Target, mode: u8) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emitter
}

/// Emits the selected integration inventory with independent output flags.
fn emit_foreign_lifetimes(target: Target, mode: u8, variant: u8) -> Emitter {
    let mut emitter = configured_emitter(target, mode);
    match variant {
        0 => owner_frames::emit(&mut emitter),
        1 => release_boundary::emit(&mut emitter),
        2 => release_boundary::emit_reference_pin_release(&mut emitter),
        _ => unreachable!(),
    }
    emitter
}

/// Fingerprints one integration family and its sorted internal-label bookkeeping.
fn foreign_lifetimes_fingerprints(variants: &[u8]) -> String {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for &variant in variants {
                let mut emitter = emit_foreign_lifetimes(Target::parse(name).unwrap(), mode, variant);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let hash = format!("{:x}", Sha256::digest(format!("{}\n{labels:?}", emitter.output())));
                actual.push(format!("{name} {mode} {variant} {hash}"));
            }
        }
    }
    actual.join("\n")
}

/// Cross-assembles every integration variant on all supported targets and output modes.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_foreign_lifetimes_assembles_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's unique source and object directory on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-foreign_lifetimes-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for variant in 0..3 {
                let emitter = emit_foreign_lifetimes(Target::parse(name).unwrap(), mode, variant);
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let source = dir.0.join(format!("{name}-{mode}-{variant}.s"));
                fs::write(&source, format!("{syntax}.text\n{}", emitter.output())).unwrap();
                let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(output.status.success(), "{name}, mode={mode}: {}",
                    String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// Preserves external record publication, unlink and GC slot adaptation.
#[test]
fn memoir_external_owner_frames_preserve_assembly() {
    assert_eq!(foreign_lifetimes_fingerprints(&[0]), include_str!("memoir_external_frames_baseline.txt").trim());
}

/// Preserves both ordinary release and pin transfer through the boxed exception accumulator.
#[test]
fn memoir_foreign_release_preserves_assembly() {
    assert_eq!(foreign_lifetimes_fingerprints(&[1, 2]), include_str!("memoir_foreign_release_baseline.txt").trim());
}

/// External publication honors host registers and hooks before exposing a complete record.
#[test]
fn memoir_external_records_use_host_context() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for mode in 0..6 {
            let record = abi::int_arg_reg_name(target, 3);
            let visitor = abi::int_arg_reg_name(target, 4);
            let data = abi::int_arg_reg_name(target, 5);
            let head = "_memoir_host_external_head";
            let cleanup = "__memoir_host_external_cleanup";
            let mut emitter = configured_emitter(target, mode);
            elephc_memoir::owner_records::publish_external(&mut emitter,
                elephc_memoir::owner_records::ExternalRecord {
                    record_address: record, visitor, data,
                    head_symbol: head, cleanup_symbol: cleanup,
                });
            let assembly = emitter.output();
            let mut expected = configured_emitter(target, mode);
            abi::emit_store_to_address(&mut expected, visitor, record, 40);
            let visitor_store = expected.output();
            let mut expected = configured_emitter(target, mode);
            abi::emit_store_to_address(&mut expected, data, record, 16);
            let data_store = expected.output();
            let mut expected = configured_emitter(target, mode);
            abi::emit_store_reg_to_symbol(&mut expected, record, head, 0);
            let head_store = expected.output();
            let published = assembly.find(&head_store).expect("publish the host record address");
            assert!(assembly.find(&visitor_store).unwrap() < published, "{name}, mode={mode}");
            assert!(assembly.find(&data_store).unwrap() < published, "{name}, mode={mode}");
            assert!(assembly.find(cleanup).unwrap() < published, "{name}, mode={mode}");
            assert!(!assembly.contains("_exc_call_frame_top"), "{name}");
            assert!(!assembly.contains("__rt_eval_owner_frame_noop"), "{name}");
            assert!(!assembly.lines().any(|line| line.trim_start().starts_with("bl ")
                || line.trim_start().starts_with("call ")), "{name}: publication cannot run PHP");
        }
    }
}

/// Foreign retirement uses host transfer and boxing hooks around protected destruction.
#[test]
fn memoir_foreign_release_uses_host_hooks() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for mode in 0..6 {
            for transfer in [false, true] {
                let mut emitter = configured_emitter(target, mode);
                elephc_memoir::foreign_release::emit_body(&mut emitter,
                    elephc_memoir::foreign_release::Context {
                        prefix: "__memoir_host_release",
                        take_owner: transfer.then_some("__memoir_host_take_staging"),
                        take_pending: "__memoir_host_unbox_exception",
                        box_pending: "__memoir_host_box_exception",
                    });
                let assembly = emitter.output();
                let take_pending = assembly.find("__memoir_host_unbox_exception").unwrap();
                let invoke = assembly.find("__rt_cleanup_invoke").unwrap();
                let box_pending = assembly.find("__memoir_host_box_exception").unwrap();
                assert!(take_pending < invoke && invoke < box_pending, "{name}, mode={mode}");
                if transfer {
                    let take_owner = assembly.find("__memoir_host_take_staging").unwrap();
                    assert!(take_pending < take_owner && take_owner < invoke, "{name}, mode={mode}");
                } else {
                    assert!(!assembly.contains("__memoir_host_take_staging"), "{name}");
                }
                assert!(assembly.contains("__memoir_host_release_begin:"), "{name}");
                assert!(assembly.contains("__memoir_host_release_return:"), "{name}");
                for fixed in ["__rt_eval_release", "__rt_eval_reference_pin_release",
                    "__rt_throwable_take_boxed", "__rt_throwable_box_owned",
                    "__rt_reference_validation_pin_take", "__rt_throw_current"] {
                    assert!(!assembly.contains(fixed), "{name}: unexpected fixed hook {fixed}");
                }
            }
        }
    }
}
