//! Purpose:
//! Checks retirement of operator-merge boxes converted to scalar user-call parameters.
//!
//! Called from:
//! - The all-target AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - An Unknown return summary cannot alias an argument box never passed to the callee.
//! - A box published before deferred checks retires through its owner slot, not the guard.

use crate::ir::{Immediate, Op};

/// All targets retire the owned Mixed operand after passing only its scalar copy.
fn verify_scalar_parameter_owners(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function scalarOwnerPick(string $text, int $index): string { return substr($text, $index, 1); }
function scalarOwnerWalk(string $text, mixed $index): string { return scalarOwnerPick($text, $index + 3); }
$xml = new SimpleXMLElement('<r/>'); echo scalarOwnerWalk('abcdefgh', 2);
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "scalarOwnerWalk").unwrap();
    let (index, call) = function.instructions.iter().enumerate().find(|(_, inst)| inst.op == Op::Call
        && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.function_names[data.as_raw() as usize] == "scalarOwnerPick")).unwrap();
    let operand = call.operands[1];
    // The `int` parameter guard converts the box and passes the int. The box it read is
    // released exactly once: by the guard itself when it reads the owned box, or, when the
    // box was published before the checks, through its owner slot once the call returns.
    let guarded_box = function.instructions.iter().find(|inst| inst.result == Some(operand)
        && inst.op == Op::ScalarParamGuard).map(|guard| guard.operands[0]);
    let releases = |value| function.instructions.iter()
        .filter(|inst| inst.op == Op::Release && inst.operands == [value]).count();
    let definition = |value| function.instructions.iter().find(|inst| inst.result == Some(value));
    let published = |boxed| definition(boxed).filter(|inst| inst.op == Op::Borrow)
        .and_then(|borrow| function.instructions[..index].iter().find(|inst|
            inst.op == Op::StoreLocal && inst.operands == [borrow.operands[0]])
            .map(|store| (borrow.operands[0], store.immediate.clone())));
    match guarded_box {
        Some(boxed) => match published(boxed) {
            None => assert_eq!(releases(boxed), 1, "{target}: the guard's converted box is released"),
            Some((rooted, slot)) => {
                assert_eq!(releases(boxed) + releases(rooted), 0,
                    "{target}: the published box is released by its owner slot only");
                assert!(function.instructions[..index].iter().any(|inst|
                    inst.op == Op::PushCallOperandOwner && inst.immediate == slot),
                    "{target}: the published box is visible to the unwinder");
                assert_eq!(function.instructions[index + 1..].iter().filter(|inst|
                    inst.op == Op::ReleaseLocalSlot && inst.immediate == slot).count(), 1,
                    "{target}: the published box retires once after the call");
                if let Some(acquire) = definition(rooted).filter(|inst| inst.op == Op::Acquire) {
                    assert_eq!(releases(acquire.operands[0]), 1,
                        "{target}: the box the publication retained is released once");
                }
            }
        },
        None => assert!(function.instructions[index + 1..].iter().any(|inst|
            inst.op == Op::Release && inst.operands == [operand]),
            "{target}: scalar conversion borrows the caller's Mixed box"),
    }
}

supported_target_cases!(scalar_parameter_owners, verify_scalar_parameter_owners);
