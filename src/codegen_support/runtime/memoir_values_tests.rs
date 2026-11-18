//! Purpose:
//! Pins value-storage helpers and runtime data before their Memoir extraction.
//!
//! Called from:
//! - Focused compiler unit tests for memory modularization.
//!
//! Key details:
//! - Includes the APM fixed-data slots from main 8fd1043dbe.
//! - All supported targets cover independent emission flags and optional memory features.
//! - Fixed-data bytes and dead-strip label bookkeeping participate in each fingerprint.

use super::{arrays, buffers, data, reference_cells, strings};
use crate::codegen_support::{emit::Emitter, platform::Target, RuntimeFeatures};
use crate::php_version::PhpVersion;
use sha2::{Digest, Sha256};

/// Preserves memory-value assembly, ABI modes, optional hooks and all fixed runtime data.
#[test]
fn memoir_values_and_memory_data_preserve_assembly() {
    let mut actual = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            for expanded in [false, true] {
                let heap = if expanded { 134_217_728 } else { 262_144 };
                let target = Target::parse(name).unwrap();
                let mut emitter = emit_value_helpers(target, mode, expanded, heap);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let fixed = data::emit_runtime_data_fixed(heap, target, PhpVersion::default());
                let value = format!("{}\n{labels:?}\n{fixed}", emitter.output());
                let hash = format!("{:x}", Sha256::digest(value));
                actual.push(format!("{name} {mode} {expanded} {hash}"));
            }
        }
    }
    assert_eq!(actual.join("\n"), include_str!("memoir_values_baseline.txt").trim(),
        "value helpers or fixed runtime data changed during extraction");
}

/// Emits the complete moved value-helper inventory through the real compiler adapter.
fn emit_value_helpers(target: Target, mode: u8, expanded: bool, heap: usize) -> Emitter {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = mode == 1 || mode == 2;
    emitter.cdylib_boundary = mode == 2 || mode == 3;
    emitter.phpext_output = mode == 4;
    emitter.dead_strip = mode == 5;
    emitter.concat_capacity = strings::concat_scratch::scratch_capacity(heap);
    let features = RuntimeFeatures {
        eval_bridge: expanded, fiber: expanded, generator: expanded,
        native_reference_validation: expanded, directory_resource: expanded,
        popen_resource: expanded, ..RuntimeFeatures::none()
    };
    arrays::emit_array_new(&mut emitter);
    arrays::emit_array_grow(&mut emitter);
    arrays::emit_hash_new(&mut emitter);
    arrays::emit_hash_grow(&mut emitter);
    arrays::emit_hash_pin(&mut emitter);
    arrays::emit_mixed_reuse_int_cell(&mut emitter);
    arrays::emit_array_free_deep(&mut emitter);
    arrays::emit_hash_free_deep(&mut emitter);
    arrays::emit_object_free_deep(&mut emitter, features);
    arrays::emit_mixed_free_deep(&mut emitter, features);
    arrays::emit_array_ensure_unique(&mut emitter);
    arrays::emit_hash_ensure_unique(&mut emitter);
    arrays::emit_array_clone_shallow(&mut emitter);
    arrays::emit_hash_clone_shallow(&mut emitter);
    arrays::emit_mixed_clone(&mut emitter);
    strings::emit_str_persist(&mut emitter, expanded);
    strings::emit_str_new_owned(&mut emitter);
    strings::emit_concat_scratch(&mut emitter);
    buffers::emit_buffer_new(&mut emitter);
    buffers::emit_buffer_free(&mut emitter);
    buffers::emit_buffer_len(&mut emitter);
    buffers::emit_buffer_resolve(&mut emitter);
    buffers::emit_buffer_try_resolve(&mut emitter);
    buffers::emit_buffer_bounds_fail(&mut emitter);
    buffers::emit_buffer_registry_fail(&mut emitter);
    buffers::emit_buffer_use_after_free(&mut emitter);
    reference_cells::emit_reference_cells(&mut emitter, expanded);
    emitter
}

/// Assembles moved helpers and complete fixed data for every supported target and mode.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn memoir_value_helpers_assemble_on_all_supported_targets() {
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        /// Removes this test's sources and objects on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
        .join(format!("memoir-value-targets-{}-{nonce}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for mode in 0..6 {
            for expanded in [false, true] {
                let target = Target::parse(name).unwrap();
                let emitter = emit_value_helpers(target, mode, expanded, 262_144);
                let prefix = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let fixed = data::emit_runtime_data_fixed(262_144, target, PhpVersion::default());
                let source = dir.0.join(format!("{name}-{mode}-{expanded}.s"));
                fs::write(&source, format!("{prefix}.text\n{}\n.data\n{fixed}", emitter.output())).unwrap();
                let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(result.status.success(), "{name}, mode={mode}, expanded={expanded}: {}",
                    String::from_utf8_lossy(&result.stderr));
            }
        }
    }
}
