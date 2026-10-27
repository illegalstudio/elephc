//! Purpose:
//! Verifies receiver exception guards follow final local storage on every target.
//!
//! Called from:
//! - The AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - Owned temporaries and Mixed projections have unwind-visible leases.
//! - Concrete borrowed parameters and immutable this remain unguarded.

use crate::ir::{Immediate, Op, RuntimeCallTarget};

/// Final storage decides whether a method receiver needs an exceptional cleanup guard.
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
            let call = function.instructions.iter().find(|inst| inst.op == Op::MethodCall).unwrap();
            let receiver = call.operands[0];
            let guarded = function.instructions.iter().any(|inst| {
                inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
                    && inst.operands.first() == Some(&receiver)
            });
            assert_eq!(guarded, matches!(name, "temporaryReceiver" | "projectedReceiver"), "{target}: {name}");
            if name == "projectedReceiver" {
                let load = function.instructions.iter().find(|inst| inst.result == Some(receiver)).unwrap();
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
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
