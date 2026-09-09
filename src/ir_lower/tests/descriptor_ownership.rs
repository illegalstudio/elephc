//! Purpose:
//! Checks descriptor-call temporary owners in lowered EIR and generated assembly.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Frame roots must survive exceptional calls and retire after ordinary returns on all targets.

use crate::ir::Op;

/// Immediate callable and raw argument-container temporaries remain visible to frame unwinding.
#[test]
fn descriptor_invocations_root_and_retire_temporary_operands_on_all_targets() {
    let source = r#"<?php
        class RootedDescriptorOwner {
            public function forward(mixed $value): mixed { return $value; }
        }
        function invoke_rooted_descriptor(RootedDescriptorOwner $owner): mixed {
            return ($owner->forward(...))(41);
        }
        echo invoke_rooted_descriptor(new RootedDescriptorOwner());
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "invoke_rooted_descriptor").unwrap();
        let invoke = function.instructions.iter().position(|inst| inst.op == Op::CallableDescriptorInvoke).unwrap();
        for operand in &function.instructions[invoke].operands {
            let store = function.instructions[..invoke].iter().find(|inst| {
                inst.op == Op::StoreLocal && inst.operands == [*operand]
            }).expect("the invocation operand needs an owning frame root");
            assert!(function.instructions[invoke + 1..].iter().any(|inst| {
                inst.op == Op::ReleaseLocalSlot && inst.immediate == store.immediate
            }), "{target}: normal return must retire the same frame root");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
