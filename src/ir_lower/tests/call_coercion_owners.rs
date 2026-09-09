//! Purpose:
//! Pins unwind registration for implicit call-argument coercions on every target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Backend-created boxes are not EIR local owners and need their own cleanup records.
//! - String loads from widened slots retire copies without consuming concrete local borrows.

/// Cleanup follows the final local representation without making borrowed string loads transferable.
#[test]
fn widened_string_consumers_release_only_detached_loads_on_all_targets() {
    use crate::ir::{Immediate, Op, ValueDef};
    use crate::types::PhpType;
    let source = r#"<?php
function widenedStringReaders(int $seed): int {
    $value = str_repeat("x", $seed);
    $copy = $value;
    $length = strlen($value);
    $same = $value === $copy;
    $value = 42;
    unset($copy);
    return $length + ($same ? 1 : 0);
}
function borrowedStringReader(string $value): int { return strlen($value); }
echo widenedStringReaders($argc), borrowedStringReader("x");
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "widenedStringReaders").unwrap();
        let mut detached_loads = 0;
        for instruction in &function.instructions {
            if instruction.op != Op::LoadLocal || instruction.result_php_type != PhpType::Str { continue; }
            let Some(Immediate::LocalSlot(slot)) = instruction.immediate else { continue; };
            if function.locals[slot.as_raw() as usize].php_type.codegen_repr() != PhpType::Mixed { continue; }
            detached_loads += 1;
            assert!(function.instructions.iter().any(|release| {
                release.op == Op::Release && release.operands == [instruction.result.unwrap()]
            }), "{target}: string copy from {slot:?} must be retired");
        }
        assert!(detached_loads >= 2, "{target}: fixture must exercise widened string reads");
        let borrowed = module.functions.iter().find(|f| f.name == "borrowedStringReader").unwrap();
        for instruction in &borrowed.instructions {
            if instruction.op != Op::Release { continue; }
            let source = borrowed.value(instruction.operands[0]).unwrap();
            let ValueDef::Instruction { inst, .. } = source.def else { continue; };
            assert_ne!(borrowed.instruction(inst).unwrap().op, Op::LoadLocal,
                "{target}: a concrete string parameter remains borrowed");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Two implicit array boxes receive paired records around the native call on every supported ABI.
#[test]
fn implicit_array_argument_boxes_have_unwind_records_on_all_targets() {
    let source = r#"<?php
function coercionTarget(array $left, array $right): int { return count($left) + count($right); }
function coercionCaller(int $seed): int {
    $left = [$seed];
    $right = ["key" => $seed];
    return coercionTarget($left, $right);
}
echo coercionCaller($argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let body = asm.split_once("@fn name=coercionCaller ").unwrap().1
            .split_once("@endfn name=coercionCaller").unwrap().0;
        let invoke = body.lines().find(|line| {
            let line = line.trim_start();
            (line.starts_with("bl ") || line.starts_with("call ")) && line.contains("coercionTarget")
        }).unwrap_or_else(|| panic!("{target}: missing native call in {body}"));
        let (before, after) = body.split_once(invoke).unwrap();
        assert_eq!(before.matches("publish temporary call operand owner").count(), 2, "{target}: {body}");
        assert_eq!(after.matches("detach temporary call operand owner").count(), 2, "{target}: {body}");
        let (inner, outer) = if target == "linux-x86_64" {
            ("mov r10, QWORD PTR [rsp + 80]", "mov r10, QWORD PTR [rsp + 16]")
        } else {
            ("ldr x10, [sp, #80]", "ldr x10, [sp, #16]")
        };
        assert!(after.find(inner).unwrap() < after.find(outer).unwrap(), "{target}: {after}");
    }
}
