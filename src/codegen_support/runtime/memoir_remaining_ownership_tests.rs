//! Purpose:
//! Freezes boxed array preparation, reference commit and string guard assembly.
//!
//! Called from:
//! - Compiler assembly regression tests.
//!
//! Key details:
//! - The fixture includes the typed-property reference emitter from main 8fd1043dbe.
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


/// Native Mixed array writers use the exact slots of their active frame on every target.
#[test]
fn attached_packed_growth_uses_native_writer_frame_slots() {
    use crate::codegen_support::{emit::Emitter, platform::Target};

    let runtime_emitters = [
        (
            "__rt_mixed_array_set",
            objects::emit_mixed_array_set as fn(&mut Emitter),
            (32, 40),
            (40, 48),
        ),
        (
            "__rt_mixed_array_gfw",
            objects::emit_mixed_array_fetch_for_write,
            (24, 32),
            (32, 40),
        ),
    ];
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for (label, emit, (arm_array, arm_index), (x86_array, x86_index)) in runtime_emitters {
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            let (_, growth) = assembly.split_once(&format!("{label}_grow_check:")).unwrap();
            let (growth, _) = growth.split_once(&format!("{label}_grow_ready:")).unwrap();
            let expected = if name == "linux-x86_64" {
                [
                    format!("mov r10, QWORD PTR [rbp - {x86_array}]"),
                    format!("mov r9, QWORD PTR [rbp - {x86_index}]"),
                    "call __rt_array_grow".into(),
                    format!("mov QWORD PTR [rbp - {x86_array}], rax"),
                    "mov r10, QWORD PTR [rbp - 8]".into(),
                    "mov QWORD PTR [r10 + 8], rax".into(),
                ]
            } else {
                [
                    format!("ldr x10, [sp, #{arm_array}]"),
                    format!("ldr x9, [sp, #{arm_index}]"),
                    "bl __rt_array_grow".into(),
                    format!("str x0, [sp, #{arm_array}]"),
                    "ldr x10, [sp, #0]".into(),
                    "str x0, [x10, #8]".into(),
                ]
            };
            let mut rest = growth;
            for instruction in expected {
                rest = rest.split_once(&instruction)
                    .unwrap_or_else(|| panic!("{name} {label} missing {instruction}: {growth}"))
                    .1;
            }
        }
    }
}
