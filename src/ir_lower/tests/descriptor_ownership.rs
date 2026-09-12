//! Purpose:
//! Checks descriptor-call temporary owners in lowered EIR and generated assembly.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Frame roots must survive exceptional calls and retire after ordinary returns on all targets.

use crate::ir::{Immediate, LocalKind, Op, Ownership};

/// Concrete descriptor strings transfer one exact owner and retire it after expression use.
#[test]
fn concrete_descriptor_string_results_are_owned_and_released_once_on_all_targets() {
    let source = r#"<?php
        class DescriptorStringResult {
            public function value(): string { return "descriptor-result"; }
        }
        function discard_descriptor_string(DescriptorStringResult $receiver): void {
            ($receiver->value(...))();
        }
        discard_descriptor_string(new DescriptorStringResult());
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "discard_descriptor_string")
            .unwrap();
        let invoke = function
            .instructions
            .iter()
            .find(|inst| inst.op == Op::CallableDescriptorInvoke)
            .expect("instance first-class callable must use the descriptor ABI");
        let result = invoke.result.expect("descriptor string invocation must produce a result");
        let metadata = function.value(result).unwrap();
        assert_eq!(metadata.php_type, crate::types::PhpType::Str, "{target}");
        assert_eq!(metadata.ownership, crate::ir::Ownership::Owned, "{target}");
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|inst| inst.op == Op::Release && inst.operands == [result])
                .count(),
            1,
            "{target}",
        );
    }
}

/// Static callable-array expression forms stage one exact descriptor string owner.
#[test]
fn static_callable_array_string_results_are_owned_and_staged_on_all_targets() {
    let source = r#"<?php
        class StaticDescriptorStringResult {
            public static function stamp(string $value = "ok"): string {
                return "[" . $value . "]";
            }
        }
        function direct_static_descriptor(): void {
            $callback = [StaticDescriptorStringResult::class, "stamp"];
            echo $callback(value: "direct");
        }
        function parenthesized_static_descriptor(): void {
            $callback = [StaticDescriptorStringResult::class, "stamp"];
            echo ($callback)("parenthesized");
        }
        function literal_static_descriptor(): void {
            echo ([StaticDescriptorStringResult::class, "stamp"])(value: "literal");
        }
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for function_name in [
            "direct_static_descriptor",
            "parenthesized_static_descriptor",
            "literal_static_descriptor",
        ] {
            let function = module
                .functions
                .iter()
                .find(|function| function.name == function_name)
                .unwrap();
            let invokes = function
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, inst)| inst.op == Op::ExprCall)
                .collect::<Vec<_>>();
            assert_eq!(invokes.len(), 1, "{target}: {function_name}");
            let (invoke_index, invoke) = invokes[0];
            let result = invoke
                .result
                .expect("static descriptor string invocation must produce a result");
            let metadata = function.value(result).unwrap();
            assert_eq!(metadata.php_type, crate::types::PhpType::Str, "{target}: {function_name}");
            assert_eq!(metadata.ownership, Ownership::Owned, "{target}: {function_name}");
            assert_eq!(invoke.result_ownership, Ownership::Owned, "{target}: {function_name}");

            let (stage_index, stage) = function.instructions[invoke_index + 1..]
                .iter()
                .enumerate()
                .find(|(_, inst)| inst.op == Op::StoreLocal && inst.operands == [result])
                .map(|(index, inst)| (invoke_index + 1 + index, inst))
                .expect("descriptor result must move into its prepublished owner slot");
            let result_slot = match stage.immediate.as_ref() {
                Some(Immediate::LocalSlot(slot)) => *slot,
                _ => panic!("{target}: {function_name}: result staging must name a local slot"),
            };
            assert_eq!(
                function.locals[result_slot.as_raw() as usize].kind,
                LocalKind::OwnedTemp,
                "{target}: {function_name}",
            );
            assert!(function.instructions[..invoke_index].iter().any(|inst| {
                inst.op == Op::PushCallOperandOwner
                    && inst.immediate == Some(Immediate::LocalSlot(result_slot))
            }), "{target}: {function_name}: result owner must be published before invocation");

            let result_pop = function.instructions[stage_index + 1..]
                .iter()
                .position(|inst| {
                    inst.op == Op::PopCallOperandOwner
                        && inst.immediate == Some(Immediate::LocalSlot(result_slot))
                })
                .map(|index| stage_index + 1 + index)
                .expect("descriptor result staging must be retired after callback cleanup");
            assert!(function.instructions[stage_index + 1..result_pop].iter().any(|inst| {
                inst.op == Op::ReleaseLocalSlot
                    && inst.immediate != Some(Immediate::LocalSlot(result_slot))
            }), "{target}: {function_name}: descriptor owner must retire while the result stays staged");
            assert!(function.instructions[result_pop + 1..].iter().any(|inst| {
                inst.op == Op::UnsetLocal
                    && inst.immediate == Some(Immediate::LocalSlot(result_slot))
            }), "{target}: {function_name}: staging slot must transfer the result back to SSA");
        }
    }
}

/// Propagated callable-parameter signatures keep direct and CUFA string results concrete.
#[test]
fn callable_parameter_descriptor_string_results_stay_owned_on_all_targets() {
    let source = r#"<?php
        function descriptor_string(string $value): string { return '[' . $value . ']'; }
        function invoke_descriptor_direct(callable $callback, array $arguments): string {
            return $callback(...$arguments);
        }
        function invoke_descriptor_cufa(callable $callback, array $arguments): string {
            return call_user_func_array($callback, $arguments);
        }
        echo invoke_descriptor_direct(descriptor_string(...), ['direct']);
        echo invoke_descriptor_cufa(descriptor_string(...), ['cufa']);
    "#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for function_name in ["invoke_descriptor_direct", "invoke_descriptor_cufa"] {
            let function = module
                .functions
                .iter()
                .find(|function| function.name == function_name)
                .unwrap();
            let invoke = function
                .instructions
                .iter()
                .find(|instruction| instruction.op == Op::CallableDescriptorInvoke)
                .expect("the callable parameter must use descriptor invocation");
            let result = invoke
                .result
                .expect("the descriptor string invocation must return a value");
            let metadata = function.value(result).unwrap();
            assert_eq!(metadata.php_type, crate::types::PhpType::Str, "{target}/{function_name}");
            assert_eq!(metadata.ownership, Ownership::Owned, "{target}/{function_name}");
            assert_eq!(invoke.result_ownership, Ownership::Owned, "{target}/{function_name}");
        }
    }
}

/// Handler registration retires internal boxes and descriptors without consuming a caller's box.
#[test]
fn handler_graphs_release_only_their_prepared_owners_on_all_targets() {
    let source = r#"<?php
function directHandlers(): void {
    set_error_handler(function(int $level, string $message): bool { return true; });
    restore_error_handler();
    set_exception_handler(function(Throwable $error): void {});
    restore_exception_handler();
}
function boxedHandler(mixed $callback): void { set_error_handler($callback); restore_error_handler(); }
directHandlers();
boxedHandler(function(int $level, string $message): bool { return true; });
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let mut registrations = 0;
        for function in &module.functions {
            for (index, inst) in function.instructions.iter().enumerate() {
                if inst.op != Op::CoreBuiltin { continue; }
                let Some(crate::ir::Immediate::I64(selector)) = inst.immediate else { continue; };
                if !matches!(crate::ir::CoreBuiltinOp::from_i64(selector), Some(
                    crate::ir::CoreBuiltinOp::SetErrorHandler | crate::ir::CoreBuiltinOp::SetExceptionHandler
                )) { continue; }
                registrations += 1;
                let cleanup = &function.instructions[index + 1..];
                assert_eq!(cleanup[0].op, Op::Release, "{target}");
                assert_eq!(cleanup[0].operands, vec![inst.operands[1]], "{target}");
                let source_op = function.value(inst.operands[0]).and_then(|value| match value.def {
                    crate::ir::ValueDef::Instruction { inst, .. } => function.instruction(inst).map(|inst| inst.op),
                    _ => None,
                });
                if source_op == Some(Op::MixedBox) {
                    assert_eq!(cleanup[1].op, Op::Release, "{target}");
                    assert_eq!(cleanup[1].operands, vec![inst.operands[0]], "{target}");
                }
            }
            if function.name == "boxedHandler" {
                assert!(!function.instructions.iter().any(|inst| inst.op == Op::MixedBox),
                    "{target}: borrowed boxed handlers must not acquire a fake boxing owner");
            }
        }
        assert_eq!(registrations, 3, "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Both forms of direct boxed calls use the existing tag-checked descriptor ABI on every target.
#[test]
fn boxed_array_read_direct_calls_lower_through_descriptors_on_all_targets() {
    let source = r#"<?php
        function boxed_targets(): array { return [fn(int $value): int => $value + 1]; }
        $callbacks = boxed_targets();
        $callback = $callbacks[0];
        echo $callback(10), $callbacks[0](20);
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let calls = module.functions.iter().flat_map(|function| {
            function.instructions.iter().filter(|inst| {
                inst.op == Op::CallableDescriptorInvoke
                    && function.value(inst.operands[0]).unwrap().php_type == crate::types::PhpType::Mixed
            })
        }).count();
        assert_eq!(calls, 2, "{target}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("mixed_callable_closure"), "{target}");
    }
}

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
            .filter(|(_, inst)| inst.op == Op::DescriptorArgSet).collect::<Vec<_>>();
        assert_eq!(writes.len(), 2, "{target}");
        for (index, write) in writes {
            let value = write.operands[2];
            let load = function.instructions.iter().find(|inst| inst.result == Some(value))
                .expect("a hash write borrows its published string operand");
            assert_eq!(load.op, Op::LoadLocal, "{target}");
            let slot = load.immediate.clone();
            assert!(function.instructions[..index].iter().any(|inst| {
                inst.op == Op::PushCallOperandOwner && inst.immediate == slot
            }), "{target}: a guard can unwind through the argument's owner");
            assert!(function.instructions[index + 1..invoke].iter().any(|inst| {
                inst.op == Op::ReleaseLocalSlot && inst.immediate == slot
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
        assert!(asm.contains("__rt_array_predicate_boxed"), "{target}");
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
        function local_return(string $value): string {
            $result = $value . "!";
            return $result;
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
        for (name, owned) in [
            ("owned_return", true),
            ("borrowed_return", false),
            ("conditional_return", false),
            ("local_return", true),
        ] {
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
        let call = function.instructions.iter().position(|inst| {
            inst.op == Op::RuntimeCall
                && matches!(inst.immediate, Some(crate::ir::Immediate::RuntimeCall(
                    crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayAll)
                        | crate::ir::RuntimeCallTarget::ProfiledFunction {
                            target: crate::ir::RuntimeFnId::ArrayAll, ..
                        },
                )))
        }).expect("missing array_all callback invocation");
        let roots = function.instructions[..call].iter().enumerate().filter(|(_, inst)| {
            inst.op == Op::PushCallOperandOwner
        }).filter(|(push, inst)| {
            !function.instructions[push + 1..call].iter().any(|candidate| {
                candidate.op == Op::PopCallOperandOwner && candidate.immediate == inst.immediate
            })
        }).map(|(_, inst)| inst.immediate.clone()).collect::<Vec<_>>();
        assert!(!roots.is_empty(), "{target}: callback operands must stay rooted during invocation");
        for slot in roots {
            let tail = &function.instructions[call + 1..];
            let pop = tail.iter().position(|inst| {
                inst.op == Op::PopCallOperandOwner && inst.immediate == slot
            }).expect("callback root must be popped after invocation");
            assert_eq!(tail[pop + 1..].iter().filter(|inst| {
                inst.op == Op::ReleaseLocalSlot && inst.immediate == slot
            }).count(), 1, "{target}: each callback root must retire exactly once");
        }
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
