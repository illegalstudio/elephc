//! Purpose:
//! Checks the lifetime proof used to remove redundant object parameter receiver leases.
//!
//! Called from:
//! - The EIR pass suite on every supported target.
//!
//! Key details:
//! - Mutations, reference aliases and final boxed storage keep their original unwind owners.
//! - Interpreted scope access disables preparation even for an otherwise unchanged parameter.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Function, Module, Op, ValueDef};
use crate::ir_passes::borrowed_receiver_roots::BorrowedReceiverRoots;
use crate::ir_passes::driver::IrPass;

/// Covers an immutable parameter and independent reasons its frame owner may be replaced.
fn fixture(target: &str) -> Module {
    crate::ir_lower::tests::lower_source_at_for_target(r#"<?php
final class ParameterReceiver {
    public function read(int $value): int { return $value; }
}
function stable(ParameterReceiver $receiver): int { return $receiver->read(1); }
function reassigned(ParameterReceiver $receiver): int {
    return $receiver->read(($receiver = new ParameterReceiver()) instanceof ParameterReceiver ? 1 : 0);
}
function referenced(ParameterReceiver &$receiver): int { return $receiver->read(1); }
function aliased(ParameterReceiver $receiver): int {
    $alias = &$receiver;
    return $receiver->read(1);
}
function widened(ParameterReceiver $receiver, int $flag): int {
    $value = $receiver->read(1);
    if ($flag > 0) { $receiver = null; }
    return $value;
}
"#, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap())
}

/// Returns the instruction producing the sole method receiver in one fixture function.
fn receiver_op(function: &Function) -> Op {
    let call = function.instructions.iter().find(|inst| inst.op == Op::MethodCall).unwrap();
    let ValueDef::Instruction { inst, .. } = function.value(call.operands[0]).unwrap().def
        else { panic!("receiver is not instruction-defined"); };
    function.instruction(inst).unwrap().op
}

/// Only immutable concrete by-value parameters lose their redundant retain on every backend.
#[test]
fn borrowed_receiver_roots_preserve_mutation_and_alias_guards_on_all_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = fixture(target);
        let pass = BorrowedReceiverRoots::new(&module);
        for function in &mut module.functions {
            if !["stable", "reassigned", "referenced", "aliased", "widened"].contains(&function.name.as_str()) { continue; }
            let before = receiver_op(function);
            let owners = function.instructions.iter().filter(|inst| inst.op == Op::PushCallOperandOwner).count();
            if function.name == "stable" { assert_eq!(before, Op::Acquire, "{target}"); }
            let changed = pass.run(function, &mut module.data);
            let stable = function.name == "stable";
            assert_eq!(changed, stable, "{target}: {}", function.name);
            assert_eq!(receiver_op(function), if stable { Op::LoadLocal } else { before }, "{target}: {}", function.name);
            assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::PushCallOperandOwner).count(), if stable { 0 } else { owners }, "{target}: {}", function.name);
            crate::ir::validate_function(function).unwrap();
            assert!(!pass.run(function, &mut module.data), "{target}: preparation must be idempotent");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Eval-capable modules retain the same owners even for a syntactically unchanged parameter.
#[test]
fn borrowed_receiver_roots_keep_eval_scope_owners() {
    let mut module = fixture("linux-x86_64");
    module.required_runtime_features.eval_bridge = true;
    let pass = BorrowedReceiverRoots::new(&module);
    let function = module.functions.iter_mut().find(|function| function.name == "stable").unwrap();
    assert!(!pass.is_applicable(function));
    assert!(!pass.run(function, &mut module.data));
    assert_eq!(receiver_op(function), Op::Acquire);
}
