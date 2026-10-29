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


/// Parking an earlier heap snapshot does not rewind a later sibling's borrowed concat bytes.
#[test]
fn alias_call_parking_keeps_sibling_concat_storage_on_all_targets() {
    let source = r#"<?php
function scratch_alias(string $left, int $size, string $right): string {
    return $left . ':' . $size . ':' . $right;
}
function scratch_call(string $base): string {
    $kept = $base . '!';
    return scratch_alias($kept . 'B', strlen($kept), $kept . 'C');
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "scratch_call").unwrap();
        // Internal return/cleanup stores can have no PHP source span. The backend
        // deliberately emits @src markers only for real spans, so pair the same
        // mapped stores before checking parking versus ordinary assignment.
        let stores = function.instructions.iter().filter(|inst| {
            inst.op == Op::StoreLocal && inst.span.is_some_and(|span| span.line > 0)
        }).collect::<Vec<_>>();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let body = assembly.split_once("@fn name=scratch_call ").unwrap().1
            .split_once("@endfn name=scratch_call").unwrap().0;
        let sections = body.split("op=store_local").skip(1)
            .map(|part| part.split("@src ").next().unwrap()).collect::<Vec<_>>();
        assert_eq!(sections.len(), stores.len(), "{target}: store annotations must match the mapped stores in this straight-line function");
        let mut parked = 0;
        let mut assigned = 0;
        for (store, section) in stores.iter().zip(sections) {
            let Some(Immediate::LocalSlot(slot)) = store.immediate else { panic!("store slot"); };
            let value = store.operands[0];
            let Some(acquire) = function.instructions.iter().find(|inst| inst.result == Some(value)) else { continue; };
            if acquire.op != Op::Acquire { continue; }
            if !function.instructions.iter().any(|inst| inst.op == Op::StrConcat
                && inst.result == acquire.operands.first().copied()) { continue; }
            match function.locals[slot.as_raw() as usize].kind {
                LocalKind::OwnedTemp => {
                    parked += 1;
                    assert!(!section.contains("concat_off"), "{target}: operand parking rewound sibling scratch storage: {section}");
                }
                LocalKind::PhpLocal => {
                    assigned += 1;
                    assert!(section.contains("concat_off"), "{target}: PHP assignment must still bound scratch growth: {section}");
                }
                _ => {}
            }
        }
        assert!(parked > 0 && assigned > 0, "{target}: both parking and ordinary assignment must be exercised");
    }
}
