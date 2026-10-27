//! Purpose:
//! Verifies owned string evaluation snapshots remain visible through alias-returning calls.
//!
//! Called from:
//! - The AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - Exceptional cleanup releases the snapshot; normal cleanup preserves alias transfer.

use crate::ir::{Immediate, LocalKind, Op};

/// A final string argument pin spans the callee and leaves normal alias comparison authoritative.
#[test]
fn alias_returning_calls_protect_string_evaluation_pins_on_all_targets() {
    let source = r#"<?php
function aliasStringInput(string $text, bool $fail): string {
    if ($fail) { throw new RuntimeException('caught'); }
    return $text;
}
function callStringAlias(string $text, bool $fail): string {
    return aliasStringInput($text, $fail);
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "callStringAlias").unwrap();
        let (call_index, call) = function.instructions.iter().enumerate().find(|(_, inst)| {
            inst.op == Op::Call && matches!(inst.immediate, Some(Immediate::Data(name))
                if module.data.function_names[name.as_raw() as usize].eq_ignore_ascii_case("aliasStringInput"))
        }).expect("alias-returning call");
        let operand = call.operands[0];
        assert!(function.instructions.iter().any(|inst| inst.result == Some(operand)
            && inst.op == Op::Acquire && inst.immediate == Some(Immediate::Bool(true))),
            "{target}: call consumes the independent evaluation snapshot");
        let store = function.instructions[..call_index].iter().rev().find(|inst| {
            inst.op == Op::StoreLocal && inst.operands == [operand]
        }).expect("snapshot parking slot");
        let Some(Immediate::LocalSlot(slot)) = store.immediate else { panic!("owner slot"); };
        assert_eq!(function.locals[slot.as_raw() as usize].kind, LocalKind::OwnedTemp);
        assert!(function.instructions[..call_index].iter().rev().take_while(|inst| {
            inst.op != Op::StoreLocal || inst.immediate != Some(Immediate::LocalSlot(slot))
        }).any(|inst| inst.op == Op::PushCallOperandOwner
            && inst.immediate == Some(Immediate::LocalSlot(slot))),
            "{target}: final snapshot must be published through invocation");
        assert!(function.instructions[call_index + 1..].iter().any(|inst| {
            inst.op == Op::UnsetLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
        }), "{target}: normal return detaches the pin without consuming the string");
        assert!(function.instructions[call_index + 1..].iter().any(|inst| {
            inst.op == Op::ReleaseUnlessAliases && inst.operands.first() == Some(&operand)
        }), "{target}: runtime alias comparison still decides the snapshot owner");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
