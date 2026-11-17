//! Purpose:
//! Freezes boxed array preparation, reference commit and string guard assembly.
//!
//! Called from:
//! - Compiler assembly regression tests.
//!
//! Key details:
//! - Covers all supported targets and six output configurations.

use super::{arrays, objects, reference_cells, strings};
use crate::codegen_support::abi;

/// Preserves assembly and internal labels captured from the original compiler at efc6f031c5.
#[test]
fn memoir_remaining_ownership_preserves_runtime_assembly() {
    use crate::codegen_support::{emit::Emitter, platform::Target};
    use sha2::{Digest, Sha256};

    let mut fingerprints = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in 0..6 {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.pic_data_refs = mode == 1 || mode == 2;
            emitter.cdylib_boundary = mode == 2 || mode == 3;
            emitter.phpext_output = mode == 4;
            emitter.dead_strip = mode == 5;
            arrays::emit_array_take_boxed(&mut emitter);
            objects::emit_mixed_cell_autovivify_array(&mut emitter);
            objects::emit_mixed_array_fetch_for_write(&mut emitter);
            reference_cells::emit_reference_cells(&mut emitter, true);
            strings::emit_str_offset_set(&mut emitter);
            emitter.label_global("__memoir_stack_guard_probe");
            let owner = abi::int_result_reg(&emitter);
            abi::emit_guard_owned_at_stack(&mut emitter, owner, "__memoir_stack_guard_cleanup", 32);
            abi::emit_unguard_owned_at_stack(&mut emitter, 32);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let hash = Sha256::digest(format!("{}\n{labels:?}", emitter.output()));
            fingerprints.push(format!("{name} {mode} {hash:x}"));
        }
    }
    assert_eq!(fingerprints.join("\n"), include_str!("memoir_remaining_ownership_baseline.txt").trim());
}
