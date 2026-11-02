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

/// A future eval barrier cannot leave an earlier throwing dynamic store's name unrooted.
fn verify_late_property_name_owner(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
class LateNameTarget { public int $value = 1; }
function lateNameWrite(bool $reject): void {
    $object = new LateNameTarget();
    $name = 'value';
    try { $object->{$name} = $reject ? 'bad' : 3; } catch (TypeError $error) {}
    $code = $reject ? '$widened = 1;' : '';
    eval($code);
    echo $name;
}
lateNameWrite(false);
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "lateNameWrite").unwrap();
    let (store_index, store) = function.instructions.iter().enumerate()
        .find(|(_, inst)| inst.op == Op::DynamicPropSet).expect("the runtime name must stay dynamic");
    let borrowed = function.instructions.iter().find(|inst| inst.result == Some(store.operands[1])).unwrap();
    assert_eq!(borrowed.op, Op::Borrow, "{target}: the write must borrow its independently rooted name");
    let rooted = borrowed.operands[0];
    let slot = function.instructions.iter().find_map(|inst| match inst.immediate {
        Some(Immediate::LocalSlot(slot)) if inst.op == Op::StoreLocal && inst.operands == [rooted] => Some(slot),
        _ => None,
    }).expect("the name must have an unwind-visible owner slot");
    assert!(function.instructions[..store_index].iter().any(|inst| inst.op == Op::PushCallOperandOwner
        && inst.immediate == Some(Immediate::LocalSlot(slot))), "{target}: publish before the store can throw");
    assert!(function.instructions[store_index + 1..].iter().any(|inst| inst.op == Op::ReleaseLocalSlot
        && inst.immediate == Some(Immediate::LocalSlot(slot))), "{target}: retire after a successful store");
}

supported_target_cases!(late_property_name_owners, verify_late_property_name_owner);
