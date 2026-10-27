//! Purpose:
//! Keeps immutable this receivers inlineable without dropping eval-widened object leases.
//!
//! Called from:
//! - The AST-to-EIR unit suite on every supported target.
//!
//! Key details:
//! - A later eval barrier changes ownership of even an earlier typed receiver load.

use crate::ir::{Immediate, Op, ValueDef};

/// Stable this loads stay borrowed, while a later eval keeps their independent owner rooted.
#[test]
fn method_receiver_roots_respect_later_eval_on_all_targets() {
    let source = r#"<?php
class ReceiverRoot {
    public function target(int $value): int { return $value; }
    public function plain(int $value): int { return $this->target($value); }
    public function guarded(int $value, string $source): int {
        $result = $this->target($value);
        eval($source);
        return $result;
    }
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for (name, expected) in [("ReceiverRoot::plain", Op::LoadLocal), ("ReceiverRoot::guarded", Op::Acquire)] {
            let method = module.class_methods.iter().find(|function| function.name == name).unwrap();
            let call = method.instructions.iter().find(|instruction| instruction.op == Op::MethodCall).unwrap();
            let ValueDef::Instruction { inst, .. } = method.value(call.operands[0]).unwrap().def
                else { panic!("{target}: receiver has no defining instruction"); };
            assert_eq!(method.instruction(inst).unwrap().op, expected, "{target}: {name}");
            if expected == Op::Acquire {
                assert!(method.instructions.iter().any(|instruction| instruction.op == Op::PushCallOperandOwner), "{target}");
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Borrowed reference-cell receivers retain a marked lease without releasing the source borrow.
#[test]
fn method_receiver_roots_pin_borrowed_ref_cells_on_all_targets() {
    let source = r#"<?php
class BorrowedReceiverRoot {
    public function target(int $value): int { return $value; }
}
function aliasRoot(BorrowedReceiverRoot $receiver): int {
    $alias = &$receiver;
    return $receiver->target(($alias = new BorrowedReceiverRoot())->target(1));
}
function refRoot(BorrowedReceiverRoot &$receiver): int {
    return $receiver->target(($receiver = new BorrowedReceiverRoot())->target(1));
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["aliasRoot", "refRoot"] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let call = function.instructions.iter().filter(|inst| inst.op == Op::MethodCall).last().unwrap();
            let ValueDef::Instruction { inst, .. } = function.value(call.operands[0]).unwrap().def
                else { panic!("{target}: receiver has no defining instruction"); };
            let pin = function.instruction(inst).unwrap();
            assert_eq!(pin.op, Op::Acquire, "{target}: {name}");
            assert_eq!(pin.immediate, Some(Immediate::Bool(true)), "{target}: {name}");
            let source = pin.operands[0];
            let ValueDef::Instruction { inst, .. } = function.value(source).unwrap().def
                else { panic!("{target}: source has no defining instruction"); };
            assert_eq!(function.instruction(inst).unwrap().op, Op::LoadRefCell, "{target}: {name}");
            assert!(!function.instructions.iter().any(|inst| inst.op == Op::Release && inst.operands == vec![source]), "{target}: {name}");
            assert!(function.instructions.iter().any(|inst| inst.op == Op::PushCallOperandOwner), "{target}: {name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
