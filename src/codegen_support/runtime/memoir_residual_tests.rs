//! Purpose:
//! Freezes assembly for the native storage helpers moved in the final Memoir pass.
//!
//! Called from:
//! - Compiler assembly regression tests.
//!
//! Key details:
//! - Covers all five supported targets and six emitter configurations.

use super::{arrays, handler_state, io, strings, system};
use crate::codegen_support::{emit::Emitter, platform::Target};
use sha2::{Digest, Sha256};

/// Computes storage fingerprints through the complete compiler adapters.
#[test]
fn memoir_residual_storage_preserves_native_assembly() {
    let mut fingerprints = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for mode in 0..6 {
            let mut emitter = Emitter::new(target);
            emitter.pic_data_refs = mode == 1 || mode == 2;
            emitter.cdylib_boundary = mode == 2 || mode == 3;
            emitter.phpext_output = mode == 4;
            emitter.dead_strip = mode == 5;
            arrays::emit_array_set_int(&mut emitter);
            arrays::emit_array_set_str(&mut emitter);
            arrays::emit_array_set_mixed(&mut emitter);
            arrays::emit_array_push_int(&mut emitter);
            arrays::emit_array_push_str(&mut emitter);
            arrays::emit_array_push_refcounted(&mut emitter);
            arrays::emit_array_merge_into(&mut emitter);
            arrays::emit_array_merge_into_refcounted(&mut emitter);
            arrays::emit_array_shift(&mut emitter);
            arrays::emit_array_unshift(&mut emitter);
            arrays::emit_hash_iter(&mut emitter);
            arrays::emit_hash_insert_owned(&mut emitter);
            handler_state::emit_handler_state(&mut emitter);
            system::emit_unserialize(&mut emitter);
            strings::emit_mbstring(&mut emitter, true, false);
            io::emit_stream_get_contents(&mut emitter);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let assembly = emitter.output();
            let hash = Sha256::digest(format!("{assembly}\n{labels:?}"));
            fingerprints.push(format!("{name} {mode} {hash:x}"));
        }
    }
    assert_eq!(fingerprints.join("\n"), include_str!("memoir_residual_baseline.txt").trim());
}
