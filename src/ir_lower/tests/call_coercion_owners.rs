//! Purpose:
//! Pins unwind registration for implicit call-argument coercions on every target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Backend-created boxes are not EIR local owners and need their own cleanup records.
//! - String loads from widened slots retire copies without consuming concrete local borrows.

/// Temporary callable arguments are rooted around independent calls, unlike aliasing returns.
#[test]
fn user_callable_argument_owners_are_unwind_visible_on_all_targets() {
    use crate::ir::Op;
    let source = r#"<?php
function runOwnedCallback(callable $callback): void { $callback(); }
function preserveCallback(callable $callback): callable { return $callback; }
function callbackOwnerCaller(int $seed): void {
    runOwnedCallback(function() use ($seed): void { echo $seed; });
}
function callbackAliasCaller(int $seed): callable {
    return preserveCallback(function() use ($seed): void { echo $seed; });
}
callbackOwnerCaller($argc);
$callback = callbackAliasCaller($argc);
$callback();
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let owner = module.functions.iter().find(|f| f.name == "callbackOwnerCaller").unwrap();
        let push = owner.instructions.iter().position(|inst| inst.op == Op::PushCallOperandOwner).unwrap();
        let call = owner.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
        let pop = owner.instructions.iter().position(|inst| inst.op == Op::PopCallOperandOwner).unwrap();
        assert!(push < call && call < pop, "{target}: protect the callee's whole activation");
        let alias = module.functions.iter().find(|f| f.name == "callbackAliasCaller").unwrap();
        assert!(!alias.instructions.iter().any(|inst| inst.op == Op::PushCallOperandOwner),
            "{target}: a passthrough return keeps its argument ownership");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_cleanup_call_operand_descriptor"), "{target}");
    }
}

/// Static type-name results cannot keep an owned boxed read alive through argument-alias suppression.
#[test]
fn gettype_releases_boxed_read_arguments_on_all_targets() {
    use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
    let source = r#"<?php
function boxedTypeName(array $items): string { return gettype($items[0]); }
echo boxedTypeName([$argc]);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "boxedTypeName").unwrap();
        let call = function.instructions.iter().find(|instruction| matches!(
            instruction.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Gettype)))
            | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                target: RuntimeFnId::Gettype, ..
            }))
        )).unwrap();
        assert!(function.instructions.iter().any(|instruction| {
            instruction.op == Op::Release && instruction.operands == [call.operands[0]]
        }), "{target}: the inspected boxed read must be retired");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Regex literal callbacks use the same descriptor adapter as dynamic names on every target.
#[test]
fn regex_literal_callbacks_adapt_raw_match_arrays_on_all_targets() {
    let source = r#"<?php
function literalRegexArray(array $matches): string { return "M" . count($matches); }
echo preg_replace_callback('/[A-Z]/', 'literalRegexArray', 'AB');
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("preg_replace_descriptor_callback_wrapper"), "{target}");
        assert!(asm.contains("__rt_callable_invoke"), "{target}");
    }
}

/// Retyping a binding retires its old slot without turning earlier string reads into boxed detaches.
#[test]
fn retyped_string_consumers_preserve_concrete_slot_owners_on_all_targets() {
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
        let mut concrete_loads = 0;
        for instruction in &function.instructions {
            if instruction.op != Op::LoadLocal || instruction.result_php_type != PhpType::Str { continue; }
            let Some(Immediate::LocalSlot(slot)) = instruction.immediate else { continue; };
            assert_eq!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(), PhpType::Str,
                "{target}: earlier reads must keep their concrete string storage");
            concrete_loads += 1;
        }
        assert!(concrete_loads >= 2, "{target}: fixture must exercise concrete string reads");
        assert!(function.instructions.iter().any(|instruction| {
            instruction.op == Op::ZeroLocalSlot && matches!(instruction.immediate,
                Some(Immediate::LocalSlot(slot))
                    if function.locals[slot.as_raw() as usize].php_type.codegen_repr() == PhpType::Str)
        }), "{target}: retiring a string binding must clear its original owner slot");
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
