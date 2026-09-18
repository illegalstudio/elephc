//! Purpose:
//! Verifies caller ownership when a concrete object argument is returned through Mixed storage.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Mixed boxing retains the object payload, so the caller's evaluation pin must retire separately.

use crate::ir::Op;

/// Object argument pins are rooted and retired independently of Mixed results on every target.
#[test]
fn object_arguments_returned_as_mixed_retire_their_evaluation_pins_on_all_targets() {
    let source = r#"<?php
class MixedObjectOwner {}
function returnObjectAsMixed(MixedObjectOwner $owner, int $later): mixed { return $owner; }
function objectArgumentLater(): int { return 1; }
function exerciseObjectMixedReturn(): void {
    $owner = new MixedObjectOwner();
    $returned = returnObjectAsMixed($owner, objectArgumentLater());
    unset($returned, $owner);
}
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
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseObjectMixedReturn")
            .expect("object Mixed return exercise function");
        let call_index = function
            .instructions
            .iter()
            .rposition(|inst| inst.op == Op::Call)
            .expect("object Mixed return call");
        let call = &function.instructions[call_index];
        let argument = call.operands[0];
        let root = function.instructions[..call_index]
            .iter()
            .find(|inst| inst.op == Op::StoreLocal && inst.operands == [argument])
            .expect("object argument must have an invocation root");
        assert_eq!(
            function.instructions[..call_index]
                .iter()
                .filter(|inst| {
                    inst.op == Op::PushCallOperandOwner && inst.immediate == root.immediate
                })
                .count(),
            1,
            "{target}: object invocation root must be published once",
        );
        for op in [Op::PopCallOperandOwner, Op::ReleaseLocalSlot] {
            assert_eq!(
                function.instructions[call_index + 1..]
                    .iter()
                    .filter(|inst| inst.op == op && inst.immediate == root.immediate)
                    .count(),
                1,
                "{target}: object invocation root must retire once: {op:?}",
            );
        }
        assert!(
            !function.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [argument]
            }),
            "{target}: rooted object must not also be released as an SSA value",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
