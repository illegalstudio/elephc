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

/// A computed name owns its early result across a throwing RHS despite a future eval barrier.
fn verify_late_property_name_owner(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
class LateNameTarget { public int $value = 1; }
function lateNameRhs(bool $reject): mixed {
    if ($reject) { throw new RuntimeException('rhs'); }
    return 3;
}
function lateComputedName(string $name): string { return $name; }
function lateNameWrite(bool $reject): void {
    $object = new LateNameTarget();
    $name = 'value';
    try { $object->{lateComputedName($name)} = $reject ? 'bad' : lateNameRhs($reject); } catch (TypeError $error) {}
    $code = $reject ? '$widened = 1;' : '';
    eval($code);
    echo $name;
}
lateNameWrite(false);
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "lateNameWrite").unwrap();
    let rhs = function.instructions.iter().position(|inst| inst.op == Op::Call
        && matches!(inst.immediate, Some(Immediate::Data(id))
            if module.data.function_names[id.as_raw() as usize] == "lateNameRhs")).unwrap();
    let computed = function.instructions.iter().find(|inst| inst.op == Op::Call
        && matches!(inst.immediate, Some(Immediate::Data(id))
            if module.data.function_names[id.as_raw() as usize] == "lateComputedName"))
        .expect("computed names must be evaluated once").result.unwrap();
    let mut names = std::collections::HashSet::from([computed]);
    let mut slots = std::collections::HashSet::new();
    for inst in &function.instructions[..rhs] {
        let related = inst.operands.iter().any(|value| names.contains(value))
            || matches!(inst.immediate, Some(Immediate::LocalSlot(slot))
                if inst.op == Op::LoadLocal && slots.contains(&slot));
        if related {
            if let Some(result) = inst.result { names.insert(result); }
            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                if inst.op == Op::StoreLocal { slots.insert(slot); }
            }
        }
    }
    assert!(function.instructions[..rhs].iter().any(|inst| inst.op == Op::PushCallOperandOwner
        && matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if slots.contains(&slot))),
        "{target}: the computed name must survive a throwing RHS before the store exists");
    let (store_index, store) = function.instructions.iter().enumerate()
        .find(|(_, inst)| inst.op == Op::DynamicPropSet).expect("the runtime name must stay dynamic");
    assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::Call
        && matches!(inst.immediate, Some(Immediate::Data(id))
            if module.data.function_names[id.as_raw() as usize] == "lateComputedName")).count(), 1,
        "{target}: the final store must reuse the captured computed name");
    assert_eq!(function.value(store.operands[1]).unwrap().php_type, crate::types::PhpType::Str);
    assert!(function.instructions[store_index + 1..].iter().any(|inst| inst.op == Op::ReleaseLocalSlot
        && matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if slots.contains(&slot))),
        "{target}: retire the early name after a successful store");
}

supported_target_cases!(late_property_name_owners, verify_late_property_name_owner);
