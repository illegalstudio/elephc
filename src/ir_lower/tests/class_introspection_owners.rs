//! Purpose:
//! Structural coverage for boxed class-introspection lowering: every `mixed` argument is
//! tag-checked before a class name is used, and every published owner record is retired.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Direct, `call_user_func`, `call_user_func_array`, first-class-callable and unpacked call
//!   forms must all reach the same validated specialization on every supported target.
//! - Owner records nest in LIFO order across every dispatch branch. Normal exits detach them;
//!   a throw leaves its live records attached for the exception unwinder to retire.
//! - Each lowered module must still generate assembly for every supported target.

use crate::codegen::platform::Target;
use crate::ir::{Function, Immediate, LocalSlotId, Op, PhpTypePredicate, Terminator};
use std::collections::HashMap;
use std::path::Path;

/// The five first-class targets boxed class introspection must lower identically for.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Every boxed introspection call form tag-checks its argument and retires its owner records.
#[test]
fn boxed_class_introspection_validates_tags_on_every_target() {
    let source = r#"<?php
class BoxedIntrospectionTarget { public int $a = 1; public function m(): void {} }
function boxedIntrospectionNames(): array { return ["first" => "BoxedIntrospectionTarget", "count" => 1]; }
function boxedIntrospectionSpread(): array { return [13 => "BoxedIntrospectionTarget"]; }
$names = boxedIntrospectionNames();
$vars = get_class_vars(...);
echo count(get_class_vars($names["first"])),
    count(call_user_func("get_class_vars", $names["first"])),
    count(call_user_func_array("get_class_vars", [$names["first"]])),
    count($vars($names["first"])),
    count(get_class_vars(...boxedIntrospectionSpread())),
    count(get_class_methods($names["first"]));
"#;
    for target in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(target).unwrap(),
        );
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap_or_else(|| panic!("{target}: main is lowered"));
        assert!(
            type_predicates(main, PhpTypePredicate::String) >= 6,
            "{target}: every boxed introspection argument is tag-checked before dispatch",
        );
        assert!(
            type_predicates(main, PhpTypePredicate::Object) >= 1,
            "{target}: get_class_methods() still admits an object tag",
        );
        assert_published_owners_are_retired(main, target);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Counts the runtime tag checks emitted for one PHP type predicate.
fn type_predicates(function: &Function, predicate: PhpTypePredicate) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| {
            inst.op == Op::TypePredicate && inst.immediate == Some(Immediate::TypePredicate(predicate))
        })
        .count()
}

/// Checks owner-stack agreement at joins and LIFO retirement on every reachable normal path.
fn assert_published_owners_are_retired(function: &Function, target: &str) {
    let mut arrivals = HashMap::new();
    let mut pending = vec![(function.entry, Vec::<LocalSlotId>::new())];
    while let Some((id, mut owners)) = pending.pop() {
        if let Some(previous) = arrivals.get(&id) {
            assert_eq!(previous, &owners, "{target}: owner stacks disagree at {id:?}");
            continue;
        }
        arrivals.insert(id, owners.clone());
        let block = function.block(id).expect("reachable block exists");
        for instruction in &block.instructions {
            let instruction = function.instruction(*instruction).expect("block instruction exists");
            if let Some(Immediate::LocalSlot(slot)) = &instruction.immediate {
                match instruction.op {
                    Op::PushCallOperandOwner => owners.push(*slot),
                    Op::PopCallOperandOwner => {
                        assert_eq!(owners.pop(), Some(*slot), "{target}: non-LIFO pop in {}", block.name);
                    }
                    _ => {}
                }
            }
        }
        let successors = match block.terminator.as_ref().expect("block is terminated") {
            Terminator::Br { target, .. } => vec![*target],
            Terminator::CondBr { then_target, else_target, .. } => vec![*then_target, *else_target],
            Terminator::Switch { cases, default, .. } => {
                cases.iter().map(|case| case.target).chain(std::iter::once(*default)).collect()
            }
            Terminator::Return { .. } => {
                assert!(owners.is_empty(), "{target}: {} returns with live operand records", function.name);
                Vec::new()
            }
            Terminator::Throw { .. } | Terminator::Fatal { .. } | Terminator::Unreachable => Vec::new(),
            Terminator::GeneratorSuspend { .. } => panic!("this fixture has no generator"),
        };
        for successor in successors {
            pending.push((successor, owners.clone()));
        }
    }
}
