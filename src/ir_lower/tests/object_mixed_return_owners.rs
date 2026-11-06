//! Purpose:
//! Verifies balanced ownership at object/Mixed call and return boundaries.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Mixed boxing retains the object payload, so the caller's evaluation pin must retire separately.
//! - Owned assignment-result cells retire only after their object return payload is secured.

use crate::ir::Op;

/// An owned assignment cell is retired after its independently owned object return is secured.
#[test]
fn owned_mixed_assignment_results_release_after_object_return_unboxing_on_all_targets() {
    let source = r#"<?php
class C {
    protected static $instance;
    public static function set($instance) { return static::$instance = $instance; }
}
var_dump(C::set(new C()));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let setter = module.class_methods.iter().find(|function| function.name == "C::set")
            .expect("static setter fixture");
        let (unbox_index, unbox) = setter.instructions.iter().enumerate().find(|(_, inst)|
            inst.op == Op::MixedUnbox && setter.value(inst.operands[0]).unwrap().ownership
                == crate::ir::Ownership::Owned).expect("owned Mixed return boundary");
        assert_eq!(setter.value(unbox.result.unwrap()).unwrap().ownership, crate::ir::Ownership::Owned,
            "{target}: the object payload must outlive its source cell");
        assert!(setter.instructions[unbox_index + 1..].iter().any(|inst|
            inst.op == Op::Release && inst.operands == unbox.operands),
            "{target}: the independent assignment result must retire after extracting its payload");
        let unbox = setter.instructions.iter().find(|inst| inst.op == Op::MixedUnbox
            && setter.value(inst.operands[0]).unwrap().ownership != crate::ir::Ownership::Owned)
            .expect("borrowed Mixed static-storage boundary");
        assert!(!setter.instructions.iter().any(|inst|
            inst.op == Op::Release && inst.operands == unbox.operands),
            "{target}: a rooted borrowed read is not an independent result cell");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

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
