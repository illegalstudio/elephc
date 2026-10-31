//! Purpose:
//! Verifies receiver exception guards follow final local storage on every target.
//!
//! Called from:
//! - The AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - Owned temporaries and Mixed projections have unwind-visible leases.
//! - Borrowed parameters receive independent roots before argument evaluation.
//! - Immutable this remains borrowed.

use crate::ir::{Immediate, LocalKind, Op, RuntimeCallTarget};

/// Final storage remains visible through independently rooted method receivers.
#[test]
fn method_receiver_guards_follow_final_storage_on_all_targets() {
    let source = r#"<?php
class GuardedMethodReceiver {
    public function fail(bool $throw = true): void {
        if ($throw) { throw new RuntimeException('caught'); }
    }
    public function borrowedThis(): void { $this->fail(); }
}
function temporaryReceiver(): void { (new GuardedMethodReceiver())->fail(); }
function boxedReceiver(mixed $receiver): void {
    if ($receiver instanceof GuardedMethodReceiver) { $receiver->fail(); }
}
function projectedReceiver(mixed $receiver, bool $throw): void {
    $receiver = new GuardedMethodReceiver();
    $receiver->fail($throw);
}
function concreteReceiver(GuardedMethodReceiver $receiver): void { $receiver->fail(); }
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["temporaryReceiver", "projectedReceiver", "boxedReceiver", "concreteReceiver"] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let (call_index, call) = function.instructions.iter().enumerate()
                .find(|(_, inst)| inst.op == Op::MethodCall).unwrap();
            let receiver = call.operands[0];
            let (store_index, store) = function.instructions[..call_index].iter().enumerate().rev()
                .find(|(_, inst)| inst.op == Op::StoreLocal && inst.operands == [receiver])
                .expect("receiver parking slot");
            let Some(Immediate::LocalSlot(root)) = store.immediate else { panic!("receiver owner slot"); };
            assert_eq!(function.locals[root.as_raw() as usize].kind, LocalKind::HiddenTemp,
                "{target}: {name}: receiver has an independent temporary owner");
            assert!(function.instructions[store_index + 1..call_index].iter().any(|inst| {
                inst.op == Op::PushCallOperandOwner && inst.immediate == Some(Immediate::LocalSlot(root))
            }), "{target}: {name}: receiver owner must span the invocation");
            let after = &function.instructions[call_index + 1..];
            let detach = after.iter().position(|inst| inst.op == Op::PopCallOperandOwner
                && inst.immediate == Some(Immediate::LocalSlot(root))).expect("normal owner detachment");
            let release = after.iter().position(|inst| inst.op == Op::ReleaseLocalSlot
                && inst.immediate == Some(Immediate::LocalSlot(root))).expect("normal receiver retirement");
            assert!(detach < release, "{target}: {name}: unlink before a destructor can throw");
            assert!(!function.instructions.iter().any(|inst| {
                inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
                    && inst.operands.first() == Some(&receiver)
            }), "{target}: {name}: the published root must not gain a duplicate receiver guard");
            if name == "projectedReceiver" {
                let acquire = function.instructions.iter().find(|inst| inst.result == Some(receiver)).unwrap();
                assert_eq!(acquire.op, Op::Acquire, "{target}: projected receiver lease");
                let load = function.instructions.iter().find(|inst| inst.result == acquire.operands.first().copied()).unwrap();
                let Some(Immediate::LocalSlot(slot)) = load.immediate else { panic!("receiver local"); };
                assert_eq!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(), crate::types::PhpType::Mixed);
                assert!(matches!(load.result_php_type, crate::types::PhpType::Object(_)));
            }
        }
        let method = module.class_methods.iter()
            .find(|function| function.name == "GuardedMethodReceiver::borrowedThis").unwrap();
        assert!(!method.instructions.iter().any(|inst| {
            inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
        }), "{target}: immutable this must stay borrowed");
        assert!(!method.instructions.iter().any(|inst| inst.op == Op::PushCallOperandOwner),
            "{target}: immutable this must not acquire a receiver root");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
