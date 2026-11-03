//! Purpose:
//! Verifies that native reference mutations reach live cells while observations own copies.
//!
//! Called from:
//! - The all-target EIR and emitter regression suite in CI.
//!
//! Key details:
//! - Raw cells select concrete writers and canonical cells select borrowed boxed writers.
//! - Later observations remain owned Mixed snapshots after both mutation paths merge.

use crate::ir::{Immediate, Op, Ownership, RuntimeCallTarget, ValueDef};

/// Append and indexed writes use live cells without changing subsequent value-copy semantics.
#[test]
fn native_reference_mutations_select_live_layouts_on_every_target() {
    let source = r#"<?php
function nativeMutationArray(int &$items, mixed $replacement): void {
    $items = $replacement; $items[] = 1; $items[1] = 2; echo gettype($items);
}
function nativeMutationString(string &$text, mixed $replacement): void {
    $text = $replacement; $text[1] = 'X'; echo gettype($text);
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "nativeMutationArray").unwrap();
        assert!(function.instructions.iter().any(|inst| inst.immediate ==
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ReferenceCellUsesRawPayload))),
            "{target}: raw cells need their concrete writer and writeback");
        let append = function.instructions.iter().find(|inst| inst.op == Op::MixedArrayAppend).unwrap();
        let receiver = function.value(append.operands[0]).unwrap();
        let ValueDef::Instruction { inst, .. } = receiver.def else { panic!("{target}: append receiver"); };
        assert_eq!(function.instruction(inst).unwrap().op, Op::LoadRefCell,
            "{target}: append must mutate the live canonical child");
        assert_eq!(receiver.ownership, Ownership::Borrowed, "{target}: mutation cannot consume the child");
        assert!(function.instructions.iter().any(|inst| inst.op == Op::LoadRefCellValue
            && inst.result_ownership == Ownership::Owned), "{target}: observations still own detached values");
        crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: native mutation layout: {error:?}"));
    }
}
