//! Purpose:
//! Verifies ownership transfer for boxed offsets consumed by native SPL intrinsic calls.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - A rooted temporary offset is detached and cleared before the consuming runtime call.
//! - The transferred SSA value is not released again after the call returns.

use crate::ir::{Immediate, Op};
use crate::types::PhpType;

/// Transfers a boxed float offset to `SplFixedArray::offsetGet` on every supported target.
#[test]
fn fixed_array_float_offset_transfers_its_root_before_runtime_dispatch() {
    let source = r#"<?php
$fixed = new SplFixedArray(2);
var_dump($fixed[1.0]);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let main = module
            .functions
            .iter()
            .find(|function| function.flags.is_main)
            .expect("main function");
        let (call_index, call) = main
            .instructions
            .iter()
            .enumerate()
            .find(|(_, instruction)| {
                instruction.op == Op::RuntimeCall
                    && instruction.operands.len() == 3
                    && matches!(
                        main.value(instruction.operands[0]).unwrap().php_type,
                        PhpType::Object(ref class) if class == "SplFixedArray"
                    )
            })
            .unwrap_or_else(|| panic!("{target}: missing fixed-array runtime read"));
        let key = call.operands[1];
        let slot = main.instructions[..call_index]
            .iter()
            .rev()
            .find_map(|instruction| {
                if instruction.op != Op::StoreLocal || instruction.operands != [key] {
                    return None;
                }
                match instruction.immediate.as_ref() {
                    Some(Immediate::LocalSlot(slot)) => Some(*slot),
                    _ => None,
                }
            })
            .unwrap_or_else(|| panic!("{target}: missing rooted offset slot"));
        let pop = main.instructions[..call_index]
            .iter()
            .position(|instruction| {
                instruction.op == Op::PopCallOperandOwner
                    && instruction.immediate == Some(Immediate::LocalSlot(slot))
            })
            .unwrap_or_else(|| panic!("{target}: missing transferred owner detach"));
        let clear = main.instructions[..call_index]
            .iter()
            .position(|instruction| {
                instruction.op == Op::UnsetLocal
                    && instruction.immediate == Some(Immediate::LocalSlot(slot))
            })
            .unwrap_or_else(|| panic!("{target}: missing transferred owner clear"));
        assert!(pop < clear && clear < call_index, "{target}");
        assert!(
            main.instructions[call_index + 1..].iter().all(|instruction| {
                instruction.op != Op::Release || instruction.operands != [key]
            }),
            "{target}: transferred offset was released again"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
