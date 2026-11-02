//! Purpose:
//! Pins late-widened string local reads across alias-capable user calls.
//!
//! Called from:
//! - The all-target AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - A future eval barrier changes the physical slot after argument lowering.
//! - An independent payload lease must already be published at the earlier call.

use crate::ir::{Immediate, Op};

/// The sole string argument has an unwind owner before its future Mixed storage is discovered.
fn verify_late_string_argument_owner(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function lateArgumentAlias(string $value): string {
    if ($value !== '') { throw new RuntimeException('stop'); }
    return $value;
}
$argument = $argc > 0 ? 'before' : '';
try { lateArgumentAlias($argument); } catch (RuntimeException $error) { echo $error->getMessage(); }
$code = $argc > 9 ? 'echo "unreachable";' : '';
eval($code);
echo $argument;
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let main = module.functions.iter().find(|function| function.name == "main").unwrap();
    let (call_index, call) = main.instructions.iter().enumerate().find(|(_, inst)|
        inst.op == Op::Call && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.function_names[data.as_raw() as usize] == "lateArgumentAlias"))
        .expect("the fixture must reach the alias-capable native call");
    let argument = call.operands[0];
    let lease = main.instructions.iter().find(|inst| inst.result == Some(argument)).unwrap();
    assert_eq!(lease.op, Op::Acquire, "{target}: a provisional load needs its own lease");
    assert_eq!(lease.immediate, Some(Immediate::Bool(true)),
        "{target}: pin the payload without consuming the source variable");
    let slot = main.instructions.iter().find_map(|inst| match inst.immediate {
        Some(Immediate::LocalSlot(slot)) if inst.op == Op::StoreLocal && inst.operands == [argument] => Some(slot),
        _ => None,
    }).expect("the lease must have storage visible to exception cleanup");
    let publish = main.instructions.iter().position(|inst| inst.op == Op::PushCallOperandOwner
        && inst.immediate == Some(Immediate::LocalSlot(slot))).unwrap();
    assert!(publish < call_index, "{target}: publish before entering the throwing callee");
    assert!(main.instructions[call_index + 1..].iter().any(|inst| inst.op == Op::UnsetLocal
        && inst.immediate == Some(Immediate::LocalSlot(slot))),
        "{target}: successful calls detach the pin before alias-aware retirement");
}

supported_target_cases!(late_string_argument_owners, verify_late_string_argument_owner);
