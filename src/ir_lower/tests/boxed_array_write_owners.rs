//! Purpose:
//! Pins the ownership contract of boxed array assignment operands.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - The boxed writer borrows EIR operands and consumes a separate backend-created box.
//! - Scoped roots cover both normal completion and same-frame exception cleanup.

/// Callable producers remain concrete scoped owners until the boxed writer has retained them.
#[test]
fn boxed_array_writes_root_and_retire_callable_producers_on_all_targets() {
    use crate::ir::{Immediate, Op};
    use crate::types::PhpType;
    let source = r#"<?php
class BoxedWriteTarget { public static function value(int $value): int { return $value; } }
function writeBoxedCallable(array &$items): void { $items[0] = BoxedWriteTarget::value(...); }
$items = [1];
writeBoxedCallable($items);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "writeBoxedCallable").unwrap();
        let (push, slot) = function.instructions.iter().enumerate().find_map(|(index, inst)| {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
            (inst.op == Op::PushCallOperandOwner
                && function.locals[slot.as_raw() as usize].php_type == PhpType::Callable)
                .then_some((index, slot))
        }).unwrap_or_else(|| panic!("{target}: the callback owner must have an explicit root"));
        let pop = function.instructions.iter().enumerate().skip(push + 1).find_map(|(index, inst)| {
            (inst.op == Op::PopCallOperandOwner
                && inst.immediate == Some(Immediate::LocalSlot(slot)))
                .then_some(index)
        }).unwrap_or_else(|| panic!("{target}: the callback owner root must be retired"));
        let write = function.instructions[push + 1..pop].iter().position(|inst| {
            inst.op == Op::RuntimeCall && inst.operands.len() == 3 && inst.result.is_none()
        });
        assert!(write.is_some(), "{target}: keep the producer alive across the write");
        assert!(function.instructions[pop + 1..].iter().any(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == Some(Immediate::LocalSlot(slot))
        }), "{target}: retire the retained callback producer");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
