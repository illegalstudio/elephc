//! Purpose:
//! Pins unwind registration for implicit call-argument coercions on every target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Backend-created boxes are not EIR local owners and need their own cleanup records.
//! - String loads from widened slots retire copies without consuming concrete local borrows.

/// Verifies that one exact call operand is stored in a final root spanning its call.
fn assert_final_operand_root_spans_call(
    function: &crate::ir::Function,
    call_index: usize,
    operand: crate::ir::ValueId,
    target: &str,
    detail: &str,
) -> crate::ir::LocalSlotId {
    use crate::ir::{Immediate, Op};

    let stores = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::StoreLocal && inst.operands == [operand])
        .collect::<Vec<_>>();
    assert_eq!(stores.len(), 1, "{target}: {detail} must have one final root store");
    let Some(Immediate::LocalSlot(slot)) = stores[0].immediate else {
        panic!("{target}: {detail} final root must name a slot");
    };
    let push = function
        .instructions
        .iter()
        .position(|inst| {
            inst.op == Op::PushCallOperandOwner
                && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .unwrap_or_else(|| panic!("{target}: {detail} final root must be published"));
    let pop = function
        .instructions
        .iter()
        .position(|inst| {
            inst.op == Op::PopCallOperandOwner
                && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .unwrap_or_else(|| panic!("{target}: {detail} final root must be detached"));
    let release = function
        .instructions
        .iter()
        .position(|inst| {
            inst.op == Op::ReleaseLocalSlot
                && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .unwrap_or_else(|| panic!("{target}: {detail} final root must be retired"));
    assert!(push < call_index && call_index < pop && pop < release, "{target}: {detail}");
    for op in [Op::PushCallOperandOwner, Op::PopCallOperandOwner, Op::ReleaseLocalSlot] {
        assert_eq!(function.instructions.iter().filter(|inst| {
            inst.op == op && inst.immediate == Some(Immediate::LocalSlot(slot))
        }).count(), 1, "{target}: {detail} must contain one {op:?}");
    }
    assert!(!function.instructions.iter().any(|inst| {
        inst.op == Op::Release && inst.operands == [operand]
    }), "{target}: {detail} final rooted SSA must not also be released directly");
    slot
}

/// Follows an owned source through its evaluation slot into the transferred SSA value.
fn assert_evaluation_owner_transfer(
    function: &crate::ir::Function,
    source: crate::ir::ValueId,
    target: &str,
) -> crate::ir::ValueId {
    use crate::ir::{Immediate, LocalKind, Op, Ownership};

    let retains = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::Acquire && inst.operands == [source])
        .collect::<Vec<_>>();
    assert_eq!(retains.len(), 1, "{target}: source must acquire one evaluation lease");
    let retained = retains[0].result.unwrap();
    let stores = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::StoreLocal && inst.operands == [retained])
        .collect::<Vec<_>>();
    assert_eq!(stores.len(), 1, "{target}: evaluation lease must have one owner slot");
    let Some(Immediate::LocalSlot(slot)) = stores[0].immediate else {
        panic!("{target}: evaluation owner must name a slot");
    };
    assert_eq!(function.locals[slot.as_raw() as usize].kind, LocalKind::OwnedTemp, "{target}");
    for op in [Op::PushCallOperandOwner, Op::PopCallOperandOwner, Op::UnsetLocal] {
        assert_eq!(function.instructions.iter().filter(|inst| {
            inst.op == op && inst.immediate == Some(Immediate::LocalSlot(slot))
        }).count(), 1, "{target}: evaluation owner must contain one {op:?}");
    }
    assert!(!function.instructions.iter().any(|inst| {
        inst.op == Op::ReleaseLocalSlot
            && inst.immediate == Some(Immediate::LocalSlot(slot))
    }), "{target}: transferred evaluation owner must not release its cleared slot");
    let loads = function
        .instructions
        .iter()
        .filter(|inst| {
            inst.op == Op::LoadLocal
                && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .collect::<Vec<_>>();
    assert_eq!(loads.len(), 2, "{target}: evaluation slot must lend once and transfer once");
    let borrowed = loads
        .iter()
        .find_map(|inst| {
            let value = inst.result?;
            function
                .value(value)
                .is_some_and(|value| value.ownership == Ownership::Borrowed)
                .then_some(value)
        })
        .expect("evaluation owner must expose one borrow");
    let transferred = loads
        .iter()
        .find_map(|inst| {
            let value = inst.result?;
            (value != borrowed).then_some(value)
        })
        .expect("evaluation owner must transfer one value");
    assert!(!function.instructions.iter().any(|inst| {
        inst.op == Op::Release && inst.operands == [borrowed]
    }), "{target}: evaluation borrow must not be released");
    assert_eq!(function.instructions.iter().filter(|inst| {
        inst.op == Op::Release && inst.operands == [source]
    }).count(), 1, "{target}: source SSA must retire once after its evaluation retain");
    assert_eq!(function.instructions.iter().filter(|inst| {
        inst.op == Op::Release && inst.operands == [transferred]
    }).count(), 1, "{target}: transferred SSA must retire once after its final retain");
    transferred
}

/// Merged callable storage is extracted and rooted before positional, named and spread calls.
#[test]
fn merged_callable_parameters_have_owned_descriptor_storage_on_all_targets() {
    use crate::ir::{Op, Ownership};
    use crate::types::PhpType;
    let source = r#"<?php
function callableFirst(): int { return 1; }
function callableSecond(): int { return 2; }
function consumeMergedCallback(callable $callback, int $suffix): int { return $callback() + $suffix; }
function invokeMergedCallback(int $choice): void {
    $callback = $choice > 0 ? callableFirst(...) : callableSecond(...);
    echo consumeMergedCallback($callback, 10);
    echo consumeMergedCallback(suffix: 20, callback: $callback);
    echo consumeMergedCallback(...[$callback, 30]);
}
invokeMergedCallback($argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "invokeMergedCallback").unwrap();
        let extractions: Vec<_> = function.instructions.iter().filter(|inst| {
            inst.op == Op::MixedUnbox && inst.result_php_type == PhpType::Callable
        }).collect();
        assert_eq!(extractions.len(), 3, "{target}: every call surface must repair the callable ABI");
        for extraction in extractions {
            let value = extraction.result.unwrap();
            assert_eq!(function.value(value).unwrap().ownership, Ownership::Owned, "{target}");
            let transferred = assert_evaluation_owner_transfer(function, value, target);
            let retained = function.instructions.iter().find(|inst| {
                inst.op == Op::Acquire && inst.operands == [transferred]
            }).expect("the transferred descriptor needs an independent invocation root");
            let operand = retained.result.unwrap();
            let call_index = function.instructions.iter().position(|inst| {
                inst.op == Op::Call && inst.operands.contains(&operand)
            }).expect("the rooted descriptor must be passed to its call");
            assert_final_operand_root_spans_call(
                function,
                call_index,
                operand,
                target,
                "extracted callable descriptor",
            );
        }
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_mixed_unbox"), "{target}");
    }
}

/// Statically resolved CUF, FCC and closure calls protect temporary arguments on every ABI.
#[test]
fn static_callable_arguments_have_unwind_roots_on_all_targets() {
    use crate::ir::Op;
    let source = r#"<?php
function consumeStaticArray(array $items): int { return count($items); }
function staticCufOwner(int $seed): int {
    return call_user_func("consumeStaticArray", [$seed]);
}
function staticFccOwner(int $seed): int {
    $callback = consumeStaticArray(...);
    return $callback([$seed]);
}
function staticClosureOwner(int $seed): int {
    $callback = function(array $items): int { return count($items); };
    return $callback([$seed]);
}
echo staticCufOwner($argc), staticFccOwner($argc), staticClosureOwner($argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["staticCufOwner", "staticFccOwner", "staticClosureOwner"] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            let call = function.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
            let operand = function.instructions[call].operands[0];
            assert_final_operand_root_spans_call(
                function,
                call,
                operand,
                target,
                &format!("{name} temporary array"),
            );
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Eval-backed class names detach their strings and retire both temporary owners on every target.
#[test]
fn eval_class_name_results_release_bridge_cells_and_strings_on_all_targets() {
    use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
    let source = r#"<?php
class BridgeNameBase {}
class BridgeNameChild extends BridgeNameBase {}
$source = 'return new BridgeNameChild();' . ' // ' . $argc;
$object = eval($source);
echo get_class($object), ":", get_parent_class($object);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let mut lookups = 0;
        for function in &module.functions {
            for call in &function.instructions {
                if !matches!(call.immediate, Some(Immediate::RuntimeCall(
                    RuntimeCallTarget::Function(RuntimeFnId::GetClass | RuntimeFnId::GetParentClass)
                    | RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::GetClass | RuntimeFnId::GetParentClass, ..
                    }
                ))) { continue; }
                lookups += 1;
                assert!(function.instructions.iter().any(|inst| {
                    inst.op == Op::Release && inst.operands == [call.result.unwrap()]
                }), "{target}: each native class-name result has a release");
            }
        }
        assert_eq!(lookups, 2, "{target}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let calls: Vec<_> = asm.match_indices("__elephc_eval_object_class_name").collect();
        assert_eq!(calls.len(), 2, "{target}: both Mixed operands must use the eval bridge");
        for (offset, _) in calls {
            let path = asm[offset..].split("eval_object_class_non_object").next().unwrap();
            let persist = path.find("__rt_str_persist").expect("detach bridge string");
            let release = path.find("__rt_decref_mixed").expect("release bridge result cell");
            assert!(persist < release, "{target}: keep the copied bytes before releasing the cell");
            assert!(!path.contains("__rt_mixed_box_bool"), "{target}: class names are not booleans");
        }
    }
}

/// Echo retires widened-slot string copies while concrete parameter reads remain borrowed.
#[test]
fn echo_retires_widened_string_reads_but_preserves_concrete_borrows() {
    use crate::ir::{Immediate, Op};
    use crate::types::PhpType;
    let source = r#"<?php
class EchoStringName {}
$source = 'return new EchoStringName(); // ' . $argc;
for ($i = 0; $i < 2; $i++) {
    $object = eval($source);
    $name = get_class($object);
    echo $name;
    unset($name, $object);
}
function echoConcreteString(string $text): void { echo $text, $text; }
echoConcreteString("borrowed");
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let mut detached_reads = 0;
        let mut borrowed_reads = 0;
        for function in &module.functions {
            for echo in function.instructions.iter().filter(|inst| inst.op == Op::EchoValue) {
                let value = echo.operands[0];
                let Some(load) = function.instructions.iter().find(|inst| inst.result == Some(value)) else {
                    continue;
                };
                if load.op != Op::LoadLocal || load.result_php_type != PhpType::Str { continue; }
                let Some(Immediate::LocalSlot(slot)) = load.immediate else { continue; };
                let released = function.instructions.iter().any(|inst| {
                    inst.op == Op::Release && inst.operands == [value]
                });
                if function.locals[slot.as_raw() as usize].php_type.codegen_repr() == PhpType::Mixed {
                    detached_reads += 1;
                    assert!(released, "{target}: a widened string read owns its detached copy");
                } else if function.name == "echoConcreteString" {
                    borrowed_reads += 1;
                    assert!(!released, "{target}: concrete string reads must retain their caller owner");
                }
            }
        }
        assert!(detached_reads > 0 && borrowed_reads > 0, "{target}: both storage paths must be exercised");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Class-name metadata cannot keep a boxed object read alive after introspection finishes.
#[test]
fn class_name_lookups_retire_boxed_read_arguments_on_all_targets() {
    use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
    let source = r#"<?php
class NameOwnerBase {}
class NameOwnerChild extends NameOwnerBase {}
function inspectNameOwner(array $items): string {
    return get_class($items[0]) . ":" . get_parent_class($items[0]);
}
echo inspectNameOwner([new NameOwnerChild()]);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "inspectNameOwner").unwrap();
        for operation in [RuntimeFnId::GetClass, RuntimeFnId::GetParentClass] {
            let call = function.instructions.iter().find(|inst| match inst.immediate {
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(id)
                    | RuntimeCallTarget::ProfiledFunction { target: id, .. })) => id == operation,
                _ => false,
            }).unwrap();
            assert!(function.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [call.operands[0]]
            }), "{target}: {operation:?} must retire its boxed argument");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Descriptor-only invocations emit one complete normalizer without requiring the eval bridge.
#[test]
fn callable_argument_normalizer_is_emitted_once_on_all_targets() {
    let source = r#"<?php
class CallableArgumentObject { public function __invoke(): void { echo "called"; } }
function consumeCallableArgument(callable $callback): void { $callback(); }
function dispatchCallableArgument(mixed $target, mixed $callback): void {
    $target($callback);
    $target(callback: $callback);
}
dispatchCallableArgument(consumeCallableArgument(...), new CallableArgumentObject());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let dispatch = module.functions.iter().find(|f| f.name == "dispatchCallableArgument").unwrap();
        assert_eq!(dispatch.params[0].php_type, crate::types::PhpType::Mixed,
            "{target}: callback validation must run through the runtime descriptor boundary");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert_eq!(asm.matches("_eir_callable_argument_normalizer:").count(), 1, "{target}");
        let normalizer = asm.split_once("_eir_callable_argument_normalizer:\n").unwrap().1;
        let descriptor_label = normalizer.lines().find(|line| {
            line.trim_end().ends_with(':') && line.contains("mixed_callable_value_descriptor_")
        }).expect("the normalizer must have an existing-descriptor branch");
        let descriptor_branch: Vec<_> = normalizer.split_once(descriptor_label).unwrap().1.lines()
            .take_while(|line| !line.trim_end().ends_with(':')).collect();
        assert_eq!(descriptor_branch.iter().filter(|line| {
            let line = line.trim_start();
            (line.starts_with("bl ") || line.starts_with("call ")) && line.contains("__rt_incref")
        }).count(), 1, "{target}: the existing descriptor acquires exactly one owner");
        assert!(asm.contains("__rt_throw_current"), "{target}: invalid callbacks remain catchable");
        assert!(!asm.contains("__elephc_eval_dynamic_callable_invoker"), "{target}: no eval dependency");
    }
}

/// Temporary callable arguments are rooted around independent calls, unlike aliasing returns.
#[test]
fn user_callable_argument_owners_are_unwind_visible_on_all_targets() {
    use crate::ir::{Immediate, LocalKind, Op, ValueDef};
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
        let call = owner.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
        assert_final_operand_root_spans_call(
            owner,
            call,
            owner.instructions[call].operands[0],
            target,
            "owned callback argument",
        );
        let alias = module.functions.iter().find(|f| f.name == "callbackAliasCaller").unwrap();
        let call = alias.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
        let callback = alias.instructions[call].operands[0];
        let ValueDef::Instruction { inst, .. } = alias.value(callback).unwrap().def else {
            panic!("{target}: aliasing callback must be an evaluation transfer");
        };
        let load = alias.instruction(inst).unwrap();
        let Some(Immediate::LocalSlot(slot)) = load.immediate else {
            panic!("{target}: aliasing callback transfer must name its evaluation slot");
        };
        assert_eq!(load.op, Op::LoadLocal, "{target}");
        assert_eq!(alias.locals[slot.as_raw() as usize].kind, LocalKind::OwnedTemp, "{target}");
        let store = alias.instructions.iter().find(|candidate| {
            candidate.op == Op::StoreLocal
                && candidate.immediate == Some(Immediate::LocalSlot(slot))
        }).expect("aliasing callback evaluation owner store");
        let retained = store.operands[0];
        let retain = alias.instructions.iter().find(|candidate| {
            candidate.op == Op::Acquire && candidate.result == Some(retained)
        }).expect("aliasing callback evaluation retain");
        let source = retain.operands[0];
        let ValueDef::Instruction { inst, .. } = alias.value(source).unwrap().def else {
            panic!("{target}: callback source must be instruction-defined");
        };
        assert_eq!(alias.instruction(inst).unwrap().op, Op::ClosureNew, "{target}");
        let push = alias.instructions.iter().position(|candidate| {
            candidate.op == Op::PushCallOperandOwner
                && candidate.immediate == Some(Immediate::LocalSlot(slot))
        }).unwrap();
        let pop = alias.instructions.iter().position(|candidate| {
            candidate.op == Op::PopCallOperandOwner
                && candidate.immediate == Some(Immediate::LocalSlot(slot))
        }).unwrap();
        assert!(push < pop && pop < call, "{target}: evaluation owner transfers before aliasing call");
        assert_eq!(alias.instructions.iter().filter(|candidate| {
            candidate.op == Op::PushCallOperandOwner
        }).count(), 1, "{target}: aliasing call has only its evaluation root");
        assert!(!alias.instructions.iter().any(|candidate| {
            candidate.op == Op::Release && candidate.operands == [callback]
        }), "{target}: passthrough return transfers the callback owner to its result");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_cleanup_call_operand_descriptor"), "{target}");
    }
}

/// Callee-owned array parameters remain rooted even when opaque dispatch makes return aliasing unknown.
#[test]
fn owned_shadow_arguments_have_unwind_roots_with_unknown_results_on_all_targets() {
    use crate::ir::{Immediate, LocalKind, Op, ValueDef};
    let source = r#"<?php
function invokeShadowTarget(callable $target, array $arguments): mixed {
    return call_user_func_array($target, $arguments);
}
function shadowArgumentCaller(callable $target): mixed {
    return invokeShadowTarget($target, [["value" => "kept"]]);
}
function retainLaterCallback(array $items, callable $callback): callable { return $callback; }
function laterCallbackCaller(int $seed): callable {
    return retainLaterCallback([$seed], function() use ($seed): void { echo $seed; });
}
function countShadowArgument(array $value): int { return count($value); }
echo shadowArgumentCaller(countShadowArgument(...));
$callback = laterCallbackCaller($argc);
$callback();
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["shadowArgumentCaller", "laterCallbackCaller"] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            let call = function.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
            let array_operand_index = if name == "shadowArgumentCaller" { 1 } else { 0 };
            let slot = assert_final_operand_root_spans_call(
                function,
                call,
                function.instructions[call].operands[array_operand_index],
                target,
                &format!("{name} owned array argument"),
            );
            assert_ne!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(), crate::types::PhpType::Callable,
                "{target}: raw callable passthrough must retain its transfer contract");
            if name == "laterCallbackCaller" {
                let callback = function.instructions[call].operands[1];
                let ValueDef::Instruction { inst, .. } = function.value(callback).unwrap().def else {
                    panic!("missing callback evaluation transfer");
                };
                let callback_load = function.instruction(inst).unwrap();
                let Some(Immediate::LocalSlot(callback_slot)) = callback_load.immediate else {
                    panic!("missing callback evaluation slot");
                };
                assert_eq!(callback_load.op, Op::LoadLocal, "{target}");
                assert_eq!(
                    function.locals[callback_slot.as_raw() as usize].kind,
                    LocalKind::OwnedTemp,
                    "{target}",
                );
                let retained = function.instructions.iter().find(|instruction| {
                    instruction.op == Op::StoreLocal
                        && instruction.immediate == Some(Immediate::LocalSlot(callback_slot))
                }).expect("callback evaluation owner store").operands[0];
                let source = function.instructions.iter().find(|instruction| {
                    instruction.op == Op::Acquire && instruction.result == Some(retained)
                }).expect("callback evaluation retain").operands[0];
                let ValueDef::Instruction { inst, .. } = function.value(source).unwrap().def else {
                    panic!("missing closure source");
                };
                assert_eq!(function.instruction(inst).unwrap().op, Op::ClosureNew, "{target}");
                let pop = function.instructions.iter().position(|instruction| {
                    instruction.op == Op::PopCallOperandOwner
                        && instruction.immediate == Some(Immediate::LocalSlot(callback_slot))
                }).unwrap();
                assert!(pop < call, "{target}: aliasing callback evaluation root transfers before the call");
                assert!(!function.instructions.iter().any(|instruction| {
                    instruction.op == Op::Release && instruction.operands == [callback]
                }), "{target}: rooting parameter zero must not renumber the returned callback");
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
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
    use crate::ir::Op;

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
        let caller = module.functions.iter().find(|function| function.name == "coercionCaller").unwrap();
        assert_eq!(caller.instructions.iter().filter(|inst| inst.op == Op::PushCallOperandOwner).count(),
            4, "{target}: both source arrays have evaluation and final EIR roots");
        assert_eq!(caller.instructions.iter().filter(|inst| inst.op == Op::PopCallOperandOwner).count(),
            4, "{target}: both source evaluation and final roots must be detached");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let body = asm.split_once("@fn name=coercionCaller ").unwrap().1
            .split_once("@endfn name=coercionCaller").unwrap().0;
        let invoke = body.lines().find(|line| {
            let line = line.trim_start();
            (line.starts_with("bl ") || line.starts_with("call ")) && line.contains("coercionTarget")
        }).unwrap_or_else(|| panic!("{target}: missing native call in {body}"));
        let (before, after) = body.split_once(invoke).unwrap();
        // Two evaluation roots protect source-order lowering, two final EIR roots span the
        // call, and two backend roots protect the implicit Mixed boxes.
        assert_eq!(before.matches("publish temporary call operand owner").count(), 6, "{target}: {body}");
        assert_eq!(after.matches("detach temporary call operand owner").count(), 4, "{target}: {body}");
        let implicit = after.split_once("op=pop_call_operand_owner").unwrap().0;
        assert_eq!(implicit.matches("detach temporary call operand owner").count(), 2, "{target}: {body}");
        let (inner, outer) = if target == "linux-x86_64" {
            ("mov r10, QWORD PTR [rsp + 80]", "mov r10, QWORD PTR [rsp + 16]")
        } else {
            ("ldr x10, [sp, #80]", "ldr x10, [sp, #16]")
        };
        assert!(after.find(inner).unwrap() < after.find(outer).unwrap(), "{target}: {after}");
    }
}
