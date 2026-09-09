//! Purpose:
//! Checks descriptor-call temporary owners in lowered EIR and generated assembly.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Frame roots must survive exceptional calls and retire after ordinary returns on all targets.

use crate::ir::Op;

/// Named containers retire copied string operands before the callback can unwind past them.
#[test]
fn named_descriptor_arguments_release_persisted_strings_before_invocation_on_all_targets() {
    let source = r#"<?php
        function named_string_target(string $first, string $second): int {
            return strlen($first) + strlen($second);
        }
        function invoke_named_strings(callable $callback): int {
            return call_user_func($callback, str_repeat("a", 24), second: str_repeat("b", 24));
        }
        echo invoke_named_strings(named_string_target(...));
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "invoke_named_strings").unwrap();
        let invoke = function.instructions.iter().position(|inst| inst.op == Op::CallableDescriptorInvoke).unwrap();
        let writes = function.instructions[..invoke].iter().enumerate()
            .filter(|(_, inst)| inst.op == Op::HashSet).collect::<Vec<_>>();
        assert_eq!(writes.len(), 2, "{target}");
        for (index, write) in writes {
            let value = write.operands[2];
            assert!(function.instructions[index + 1..invoke].iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [value]
            }), "{target}: copied argument must be retired before invoking the callback");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Callback roots preserve callable-array provenance and single-word object predicate arguments.
#[test]
fn retained_callback_arrays_and_object_predicates_lower_on_all_targets() {
    let source = r#"<?php
        class RootedCallbackTarget {
            public function sum(int $carry, int $value): int { return $carry + $value; }
            public static function sumStatic(int $carry, int $value): int { return $carry + $value; }
        }
        class RootedPredicateInput { public function __destruct() { echo "released"; } }
        function rooted_predicate(RootedPredicateInput $value): bool { return true; }
        $target = new RootedCallbackTarget();
        $instance = [$target, "sum"];
        $static = ["RootedCallbackTarget", "sumStatic"];
        echo array_reduce([1, 2], $instance, 0), array_reduce([1, 2], $static, 0);
        echo array_all([new RootedPredicateInput()], rooted_predicate(...));
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_array_reduce"), "{target}");
        assert!(asm.contains("__rt_array_find_any_all"), "{target}");
        assert!(asm.contains("__rt_cleanup_call_operand_owner"), "{target}");
    }
}

/// Ownership proofs distinguish persisted returns from borrowed parameters and mixed return paths.
#[test]
fn descriptor_string_return_ownership_requires_every_return_to_transfer_an_owner() {
    let source = r#"<?php
        function owned_return(string $value): string { return $value . "!"; }
        function borrowed_return(string $value): string { return $value; }
        function conditional_return(string $value, bool $copy): string {
            if ($copy) { return $value . "!"; }
            return $value;
        }
        function invoke_return(callable $callback, string $value): string {
            return call_user_func($callback, $value);
        }
        echo invoke_return(owned_return(...), "a");
        echo invoke_return(borrowed_return(...), "b");
        echo conditional_return("c", $argc > 1);
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for (name, owned) in [("owned_return", true), ("borrowed_return", false), ("conditional_return", false)] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            assert_eq!(crate::codegen::function_returns_owned_string(function), owned, "{target}: {name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Named argument boxes remain owned by EIR while the backend consumes an independent retain.
#[test]
fn named_descriptor_argument_boxes_have_scoped_caller_owners_on_all_targets() {
    let source = r#"<?php
        function named_owned_target(int $value): int { return $value; }
        function named_owned_invoke(callable $callback): mixed {
            return call_user_func($callback, value: 17);
        }
        echo named_owned_invoke(named_owned_target(...));
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "named_owned_invoke").unwrap();
        let invoke = function.instructions.iter().position(|inst| inst.op == Op::CallableDescriptorInvoke).unwrap();
        let argument = function.instructions[invoke].operands[1];
        assert_eq!(function.value(argument).unwrap().php_type, crate::types::PhpType::Mixed, "{target}");
        let store = function.instructions[..invoke].iter().find(|inst| {
            inst.op == Op::StoreLocal && inst.operands == [argument]
        }).expect("named argument box needs a caller-owned root");
        for (slice, expected) in [
            (&function.instructions[..invoke], Op::PushCallOperandOwner),
            (&function.instructions[invoke + 1..], Op::PopCallOperandOwner),
            (&function.instructions[invoke + 1..], Op::ReleaseLocalSlot),
        ] {
            assert!(slice.iter().any(|inst| inst.op == expected && inst.immediate == store.immediate), "{target}: {expected:?}");
        }
        assert!(!function.instructions[invoke + 1..].iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [argument]
        }), "{target}: only the root owns the original argument box after invocation");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_callable_invoke_owned_args"), "{target}");
    }
}

/// Callback builtin inputs receive an unwind scope as well as normal-path slot retirement.
#[test]
fn callback_builtin_operands_have_unwind_scopes_on_all_targets() {
    let source = r#"<?php
        function callback_root_predicate(int $value): bool { return $value > 0; }
        function invoke_callback_root(callable $callback): bool { return array_all([1, 2, 3], $callback); }
        echo invoke_callback_root(callback_root_predicate(...));
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "invoke_callback_root").unwrap();
        let push = function.instructions.iter().position(|inst| inst.op == Op::PushCallOperandOwner).unwrap();
        let pop = function.instructions.iter().position(|inst| inst.op == Op::PopCallOperandOwner).unwrap();
        assert!(push < pop, "{target}");
        assert!(function.instructions[push + 1..pop].iter().any(|inst| inst.op == Op::RuntimeCall), "{target}");
        assert!(function.instructions[pop + 1..].iter().any(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == function.instructions[push].immediate
        }), "{target}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_cleanup_call_operand_owner"), "{target}");
    }
}

/// Both borrowed and runtime-created descriptors use bounded cleanup for normalized arguments.
#[test]
fn normalized_descriptor_calls_use_owned_argument_boundaries_on_all_targets() {
    let source = r#"<?php
        class NormalizedDescriptorReceiver { public function ping(string $value): int { return 7; } }
        function invoke_normalized_descriptor(callable $callback, NormalizedDescriptorReceiver $receiver, string $method): void {
            $arguments = ["input"];
            echo call_user_func_array($callback, $arguments);
            echo call_user_func_array([$receiver, $method], $arguments);
        }
        $receiver = new NormalizedDescriptorReceiver();
        invoke_normalized_descriptor($receiver->ping(...), $receiver, "ping");
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_callable_invoke_owned_args"), "{target}");
        assert!(asm.contains("__rt_callable_invoke_owned_descriptor_args"), "{target}");
    }
}

/// Dynamic calls retain receiver roots and copy borrowed method names into their owning temporaries.
#[test]
fn dynamic_method_calls_root_receiver_borrows_on_all_targets() {
    let source = r#"<?php
        class DynamicRootedReceiver { public function ping(): void { echo "p"; } }
        function invoke_dynamic_root(DynamicRootedReceiver $receiver, string $method): void {
            $receiver->$method();
        }
        invoke_dynamic_root(new DynamicRootedReceiver(), "ping");
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "invoke_dynamic_root").unwrap();
        assert!(function.instructions.iter().any(|store| {
            if store.op != Op::StoreLocal { return false; }
            let Some(value) = function.value(store.operands[0]) else { return false; };
            if value.php_type.codegen_repr() != crate::types::PhpType::Str { return false; }
            let crate::ir::ValueDef::Instruction { inst, .. } = value.def else { return false; };
            function.instruction(inst).is_some_and(|producer| producer.op == Op::Acquire)
        }), "{target}: a borrowed selector cannot be stored as a second unretained owner");
        let store = function.instructions.iter().find(|inst| {
            inst.op == Op::StoreLocal && matches!(
                function.value(inst.operands[0]).unwrap().php_type,
                crate::types::PhpType::Object(_)
            )
        }).expect("the receiver needs a frame root");
        let crate::ir::ValueDef::Instruction { inst, .. } = function.value(store.operands[0]).unwrap().def else {
            panic!("{target}: receiver root must be an acquired borrow");
        };
        assert_eq!(function.instruction(inst).unwrap().op, Op::Acquire, "{target}");
        let invoke = function.instructions.iter().position(|inst| inst.op == Op::CallableDescriptorInvoke).unwrap();
        assert!(function.instructions[invoke + 1..].iter().any(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == store.immediate
        }), "{target}: normal invocation must retire its receiver root");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Immediate callable and raw argument-container temporaries remain visible to frame unwinding.
#[test]
fn descriptor_invocations_root_and_retire_temporary_operands_on_all_targets() {
    let source = r#"<?php
        class RootedDescriptorOwner {
            public function forward(mixed $value): mixed { return $value; }
        }
        function invoke_rooted_descriptor(RootedDescriptorOwner $owner): mixed {
            return ($owner->forward(...))(41);
        }
        echo invoke_rooted_descriptor(new RootedDescriptorOwner());
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "invoke_rooted_descriptor").unwrap();
        let invoke = function.instructions.iter().position(|inst| inst.op == Op::CallableDescriptorInvoke).unwrap();
        for operand in &function.instructions[invoke].operands {
            let store = function.instructions[..invoke].iter().find(|inst| {
                inst.op == Op::StoreLocal && inst.operands == [*operand]
            }).expect("the invocation operand needs an owning frame root");
            assert!(function.instructions[invoke + 1..].iter().any(|inst| {
                inst.op == Op::ReleaseLocalSlot && inst.immediate == store.immediate
            }), "{target}: normal return must retire the same frame root");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
