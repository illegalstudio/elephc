//! Purpose:
//! Verifies EIR owner publication while source call arguments are still being evaluated.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - By-value owners are visible to unwinding before later arguments execute.
//! - By-reference places are never replaced by detached evaluation temporaries.

/// Lowers the argument-owner regression program for one supported target.
fn lower_for(source: &str, target: &str) -> crate::ir::Module {
    super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    )
}

/// Verifies one evaluation borrow is backed by one stored lease with one retirement path.
fn assert_evaluation_owner_retires_once(
    function: &crate::ir::Function,
    borrowed: crate::ir::ValueId,
    slot: crate::ir::LocalSlotId,
    target: &str,
) {
    use crate::ir::{Immediate, Op, Ownership};

    assert_eq!(function.value(borrowed).unwrap().ownership, Ownership::Borrowed, "{target}");
    let borrow = function.instructions.iter().find(|inst| {
        inst.op == Op::Borrow && inst.result == Some(borrowed)
    }).expect("evaluation borrow instruction");
    let stored = borrow.operands[0];
    let stores = function
        .instructions
        .iter()
        .filter(|inst| {
            inst.op == Op::StoreLocal
                && inst.operands == [stored]
                && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .collect::<Vec<_>>();
    assert_eq!(stores.len(), 1, "{target}: evaluation owner slot must be stored once");
    assert!(function.instructions.iter().any(|inst| {
        inst.op == Op::Acquire
            && inst.result == Some(stored)
            && inst.immediate == Some(Immediate::Bool(true))
    }), "{target}: evaluation owner slot must store a marked lifetime pin");

    let pushes = function.instructions.iter().filter(|inst| {
        inst.op == Op::PushCallOperandOwner
            && inst.immediate == Some(Immediate::LocalSlot(slot))
    }).count();
    let pops = function.instructions.iter().filter(|inst| {
        inst.op == Op::PopCallOperandOwner
            && inst.immediate == Some(Immediate::LocalSlot(slot))
    }).count();
    assert_eq!(pushes, pops, "{target}: evaluation owner publication must balance");
    let unsets = function.instructions.iter().filter(|inst| {
        inst.op == Op::UnsetLocal
            && inst.immediate == Some(Immediate::LocalSlot(slot))
    }).count();
    let slot_releases = function.instructions.iter().filter(|inst| {
        inst.op == Op::ReleaseLocalSlot
            && inst.immediate == Some(Immediate::LocalSlot(slot))
    }).count();
    let value_releases = function.instructions.iter().filter(|inst| {
        inst.op == Op::Release && inst.operands == [stored]
    }).count();
    assert_eq!(unsets + slot_releases, 1, "{target}: evaluation owner must transfer or retire once");
    assert_eq!(slot_releases + value_releases, 1, "{target}: stored lease must have one retirement");
    assert!(!function.instructions.iter().any(|inst| {
        inst.op == Op::Release && inst.operands == [borrowed]
    }), "{target}: borrowed evaluation view must not be released");
}

/// Direct, static and instance calls publish an owned first argument before a later call runs.
#[test]
fn by_value_arguments_publish_owners_before_later_throwing_arguments_on_all_targets() {
    use crate::ir::Op;

    let source = r#"<?php
class ArgumentSource { public mixed $text = "owned"; }
class ArgumentTargets {
    public static function stat(int $later, string $text): void {}
    public function inst(int $later, string $text): void {}
}
function directTarget(int $later, string $text): void {}
function throwLater(): int { throw new RuntimeException("stop"); }
function exerciseArguments(ArgumentSource $source, ArgumentTargets $target): void {
    try { directTarget(text: $source->text, later: throwLater()); } catch (RuntimeException $e) {}
    try { ArgumentTargets::stat(text: $source->text, later: throwLater()); } catch (RuntimeException $e) {}
    try { $target->inst(text: $source->text, later: throwLater()); } catch (RuntimeException $e) {}
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseArguments")
            .unwrap();
        for line in [10, 11, 12] {
            let on_line = function
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, inst)| inst.span.is_some_and(|span| span.line == line))
                .collect::<Vec<_>>();
            let push = on_line
                .iter()
                .find(|(_, inst)| inst.op == Op::PushCallOperandOwner)
                .map(|(index, _)| *index)
                .unwrap_or_else(|| panic!("{target}: line {line} did not publish the first argument"));
            let later_call = on_line
                .iter()
                .find(|(index, inst)| *index > push && inst.op == Op::Call)
                .map(|(index, _)| *index)
                .unwrap_or_else(|| panic!("{target}: line {line} has no later argument call"));
            let pop = on_line
                .iter()
                .find(|(index, inst)| *index > later_call && inst.op == Op::PopCallOperandOwner)
                .map(|(index, _)| *index)
                .unwrap_or_else(|| panic!("{target}: line {line} did not retire its evaluation root"));
            assert!(push < later_call && later_call < pop, "{target}: line {line}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A by-reference first argument remains a caller place while a later argument throws.
#[test]
fn by_reference_first_argument_is_not_detached_into_an_evaluation_owner() {
    use crate::ir::Op;

    let source = r#"<?php
function byRefTarget(string &$text, int $later): void {}
function throwAfterRef(): int { throw new RuntimeException("stop"); }
function exerciseByRef(string &$text): void {
    try { byRefTarget($text, throwAfterRef()); } catch (RuntimeException $e) {}
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseByRef")
            .unwrap();
        assert!(!function.instructions.iter().any(|inst| {
            inst.span.is_some_and(|span| span.line == 5)
                && inst.op == Op::PushCallOperandOwner
        }));
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A widened local keeps its exact reference-place marker while its incidental string view is rooted.
#[test]
fn widened_string_reference_place_roots_only_its_prethrow_value_view() {
    use crate::ir::{Immediate, Op};
    use crate::types::PhpType;

    let source = r#"<?php
function referenceViewTarget(string &$text, int $later): void {}
function throwAfterReferenceView(): int { throw new RuntimeException("stop"); }
$text = "reference-place";
try { referenceViewTarget($text, throwAfterReferenceView()); }
catch (RuntimeException $error) {}
unset($text);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap();
        let source_slot = function
            .locals
            .iter()
            .position(|local| local.name.as_deref() == Some("text"))
            .map(|index| crate::ir::LocalSlotId::from_raw(index as u32))
            .expect("reference source slot");
        assert_eq!(
            function.locals[source_slot.as_raw() as usize].php_type.codegen_repr(),
            PhpType::Mixed,
            "{target}: unset must expose the final widened-storage case",
        );
        let (load_index, place) = function
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| {
                (inst.op == Op::LoadLocal
                    && inst.immediate == Some(Immediate::LocalSlot(source_slot)))
                    .then_some((index, inst.result?))
            })
            .expect("reference-place load");
        let (acquire_index, protected) = function
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| {
                (index > load_index
                    && inst.op == Op::Acquire
                    && inst.operands == [place]
                    && inst.immediate == Some(Immediate::Bool(true)))
                    .then_some((index, inst.result?))
            })
            .expect("protected reference value view");
        let (store_index, owner_slot) = function
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| {
                if index <= acquire_index || inst.op != Op::StoreLocal || inst.operands != [protected] {
                    return None;
                }
                let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
                Some((index, slot))
            })
            .expect("protected view owner store");
        assert_ne!(owner_slot, source_slot, "{target}: owner slot must not replace the caller place");
        let push_index = function
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| {
                (index > store_index
                    && inst.op == Op::PushCallOperandOwner
                    && inst.immediate == Some(Immediate::LocalSlot(owner_slot)))
                    .then_some(index)
            })
            .expect("prethrow owner publication");
        let release_index = function
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| {
                (index > push_index && inst.op == Op::Release && inst.operands == [place])
                    .then_some(index)
            })
            .expect("detached value-view transfer");
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|inst| inst.op == Op::Release && inst.operands == [place])
                .count(),
            1,
            "{target}: the incidental value view must be released exactly once",
        );
        let throw_index = function
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| {
                (index > release_index && inst.op == Op::Call && inst.operands.is_empty())
                    .then_some(index)
            })
            .expect("later throwing argument call");
        let target_call = function
            .instructions
            .iter()
            .enumerate()
            .find(|(index, inst)| {
                *index > throw_index && inst.op == Op::Call && inst.operands.first() == Some(&place)
            })
            .expect("by-reference target call");
        assert!(
            function.instructions[push_index + 1..throw_index]
                .iter()
                .all(|inst| inst.op != Op::PopCallOperandOwner),
            "{target}: the protected view must remain unwind-visible through the later call",
        );
        assert!(
            function.instructions[throw_index + 1..target_call.0]
                .iter()
                .any(|inst| {
                    inst.op == Op::PopCallOperandOwner
                        && inst.immediate == Some(Immediate::LocalSlot(owner_slot))
                }),
            "{target}: successful evaluation must convert the protected view to an intermediate",
        );
        assert!(function.instructions[target_call.0 + 1..].iter().any(|inst| {
            inst.op == Op::ReleaseLocalSlot
                && inst.immediate == Some(Immediate::LocalSlot(owner_slot))
        }), "{target}: the intermediate owner must retire after the call");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A by-reference argument before a positional spread remains an invoker reference place.
#[test]
fn by_reference_argument_before_spread_is_not_detached_on_all_targets() {
    use crate::ir::Op;

    let source = r#"<?php
function spreadAfterRef(): int { return 1; }
function exerciseByRefSpread(callable $callback, string &$text): void {
    $callback($text, ...[spreadAfterRef()]);
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseByRefSpread")
            .unwrap();
        let reference_args = function
            .instructions
            .iter()
            .filter(|inst| inst.op == Op::InvokerRefArg)
            .collect::<Vec<_>>();
        assert_eq!(
            reference_args.len(),
            1,
            "{target}: by-reference prefix must remain an invoker reference marker",
        );
        let reference = reference_args[0].result.expect("invoker reference result");
        assert!(!function.instructions.iter().any(|inst| {
            inst.op == Op::Borrow && inst.operands == [reference]
        }), "{target}: by-reference prefix must not become an evaluation borrow");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A dynamic spread result stays published while a later named argument is evaluated.
#[test]
fn spread_source_owner_precedes_later_named_argument_call() {
    use crate::ir::Op;

    let source = r#"<?php
function makeSpread(string $text): array { return [$text]; }
function spreadTarget(string $text, int $later): void {}
function throwAfterSpread(): int { throw new RuntimeException("stop"); }
function exerciseSpread(string $text): void {
    try { spreadTarget(...makeSpread($text), later: throwAfterSpread()); } catch (RuntimeException $e) {}
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseSpread")
            .unwrap();
        let instructions = function.instructions.as_slice();
        let pushes = instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                inst.span.is_some_and(|span| span.line == 6)
                    && inst.op == Op::PushCallOperandOwner
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let calls = instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                inst.span.is_some_and(|span| span.line == 6) && inst.op == Op::Call
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert!(calls.len() >= 2, "spread maker and later argument must both be calls");
        assert!(pushes.iter().any(|push| calls[0] < *push && *push < calls[1]));
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Named Mixed coercions borrow evaluation slots and never release the ledger's stored lease.
#[test]
fn mixed_named_string_and_callable_coercions_do_not_double_retire_evaluation_owners() {
    use crate::ir::{Immediate, LocalKind, Op, Ownership};

    let source = r#"<?php
function evaluationCallbackFirst(): int { return 1; }
function evaluationCallbackSecond(): int { return 2; }
class MixedArgumentSource {
    public mixed $text = "owned";
}
function takeNamedString(int $later, string $text): void {}
function takeNamedCallable(int $later, callable $callback): void {}
function exerciseMixedNamed(MixedArgumentSource $source, int $choice, string $code): void {
    $callback = $choice > 0 ? evaluationCallbackFirst(...) : evaluationCallbackSecond(...);
    eval($code);
    takeNamedString(text: $source->text, later: 1);
    takeNamedCallable(callback: $callback, later: 2);
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseMixedNamed")
            .unwrap();
        let borrowed_evaluation_values = function
            .instructions
            .iter()
            .filter_map(|inst| {
                let value = inst.result?;
                if inst.op != Op::Borrow {
                    return None;
                }
                let stored = *inst.operands.first()?;
                let slot = function.instructions.iter().find_map(|store| {
                    let Immediate::LocalSlot(slot) = store.immediate.as_ref()? else {
                        return None;
                    };
                    (store.op == Op::StoreLocal
                        && store.operands == [stored]
                        && function.locals[(*slot).as_raw() as usize].kind == LocalKind::OwnedTemp)
                    .then_some(*slot)
                })?;
                function.value(value)
                    .is_some_and(|value| value.ownership == Ownership::Borrowed)
                    .then_some((value, slot))
            })
            .collect::<Vec<_>>();
        assert!(borrowed_evaluation_values.len() >= 2, "{target}");
        for (borrowed, slot) in borrowed_evaluation_values {
            assert_evaluation_owner_retires_once(function, borrowed, slot, target);
        }
        assert!(function.instructions.iter().any(|inst| inst.op == Op::MixedUnbox), "{target}");
        assert!(function.instructions.iter().any(|inst| {
            inst.op == Op::Cast && inst.result_php_type == crate::types::PhpType::Str
        }), "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Spread staging consumes only a borrow while its published source owner remains in the ledger.
#[test]
fn spread_temp_store_does_not_consume_the_evaluation_owner_lease() {
    use crate::ir::{Immediate, LocalKind, Op, Ownership};

    let source = r#"<?php
function createOwnedSpread(string $text): array { return [$text]; }
function createOwnedSpreadLater(): int { return 1; }
function acceptOwnedSpread(string $text, int $later): void {}
function exerciseOwnedSpread(string $text): void {
    acceptOwnedSpread(...createOwnedSpread($text), later: createOwnedSpreadLater());
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseOwnedSpread")
            .unwrap();
        let borrowed_spread = function.instructions.iter().find_map(|inst| {
            let value = inst.result?;
            if inst.op != Op::Borrow {
                return None;
            }
            let stored = *inst.operands.first()?;
            let slot = function.instructions.iter().find_map(|store| {
                let Immediate::LocalSlot(slot) = store.immediate.as_ref()? else {
                    return None;
                };
                (store.op == Op::StoreLocal
                    && store.operands == [stored]
                    && function.locals[(*slot).as_raw() as usize].kind == LocalKind::OwnedTemp)
                .then_some(*slot)
            })?;
            function.value(value).is_some_and(|value| {
                    value.ownership == Ownership::Borrowed
                        && value.php_type.codegen_repr().is_php_array()
                })
            .then_some((value, slot))
        }).expect("spread evaluation must expose a borrowed owner view");
        assert!(!function.instructions.iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [borrowed_spread.0]
        }), "{target}: spread staging must not release the borrowed ledger value");
        assert_evaluation_owner_retires_once(
            function,
            borrowed_spread.0,
            borrowed_spread.1,
            target,
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Dynamic descriptor containers are published before element and spread evaluation.
#[test]
fn descriptor_argument_containers_cover_throwing_and_growing_sources_on_all_targets() {
    use crate::ir::{Immediate, Op};

    let source = r#"<?php
function descriptorArgument(): mixed { return "owned"; }
function descriptorSpread(): array { return [1, 2, 3, 4]; }
function exerciseDescriptorOwners(callable $callback): void {
    $callback(descriptorArgument());
    $callback(...descriptorSpread());
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseDescriptorOwners")
            .unwrap();
        let containers = function
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(array_index, inst)| {
                (inst.op == Op::ArrayNew).then_some((array_index, inst.result?))
            })
            .collect::<Vec<_>>();
        assert_eq!(containers.len(), 2, "{target}: both descriptor calls need containers");
        for (ordinal, (array_index, container)) in containers.into_iter().enumerate() {
            let (store_index, slot) = function
                .instructions
                .iter()
                .enumerate()
                .find_map(|(index, inst)| {
                    if index <= array_index || inst.op != Op::StoreLocal {
                        return None;
                    }
                    let stored = *inst.operands.first()?;
                    let acquire = function.instructions.iter().any(|producer| {
                        producer.op == Op::Acquire
                            && producer.result == Some(stored)
                            && producer.operands == [container]
                    });
                    let Immediate::LocalSlot(slot) = inst.immediate.as_ref()? else {
                        return None;
                    };
                    acquire.then_some((index, *slot))
                })
                .expect("descriptor container owner store");
            let push_index = function
                .instructions
                .iter()
                .enumerate()
                .find_map(|(index, inst)| {
                    (index > store_index
                        && inst.op == Op::PushCallOperandOwner
                        && inst.immediate == Some(Immediate::LocalSlot(slot)))
                    .then_some(index)
                })
                .expect("descriptor container publication");
            let argument_call = function
                .instructions
                .iter()
                .enumerate()
                .find_map(|(index, inst)| {
                    (index > push_index && inst.op == Op::Call).then_some(index)
                })
                .expect("descriptor argument call");
            let pop_index = function
                .instructions
                .iter()
                .enumerate()
                .find_map(|(index, inst)| {
                    (index > argument_call
                        && inst.op == Op::PopCallOperandOwner
                        && inst.immediate == Some(Immediate::LocalSlot(slot)))
                    .then_some(index)
                })
                .expect("descriptor construction owner retirement");
            assert!(
                array_index < store_index
                    && store_index < push_index
                    && push_index < argument_call
                    && argument_call < pop_index,
                "{target}: descriptor owner must span argument evaluation",
            );
            let argument_line = ordinal as u32 + 5;
            let mutations = function
                .instructions
                .iter()
                .filter(|inst| {
                    inst.op == Op::ArrayPush
                        && inst
                            .span
                            .is_some_and(|span| span.line == argument_line)
                })
                .collect::<Vec<_>>();
            assert!(!mutations.is_empty(), "{target}: descriptor container must be populated");
            for push in mutations {
                let operand = push.operands[0];
                assert!(function.instructions.iter().any(|load| {
                    load.op == Op::LoadLocal
                        && load.result == Some(operand)
                        && load.immediate == Some(Immediate::LocalSlot(slot))
                }), "{target}: container mutation must write growth back to its published slot");
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Nested constructor and descriptor calls cannot attach roots to their outer call's ledger.
#[test]
fn nested_call_consumers_cannot_steal_outer_argument_evaluation_roots() {
    use crate::ir::Op;

    let source = r#"<?php
class NestedArgument { public function __construct(int $value) {} }
class NestedSource { public mixed $text = "owned"; }
function nestedThrow(): int { throw new RuntimeException("stop"); }
function nestedSink(string $text, mixed $later): void {}
function nestedDescriptorTarget(int $value): int { return $value; }
function exerciseNestedCalls(NestedSource $source): void {
    $callback = nestedDescriptorTarget(...);
    try { nestedSink($source->text, new NestedArgument(nestedThrow())); } catch (RuntimeException $e) {}
    try { nestedSink($source->text, $callback(nestedThrow())); } catch (RuntimeException $e) {}
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "exerciseNestedCalls")
            .unwrap();
        for line in [9, 10] {
            let on_line = function
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, inst)| inst.span.is_some_and(|span| span.line == line))
                .collect::<Vec<_>>();
            let push = on_line
                .iter()
                .find(|(_, inst)| inst.op == Op::PushCallOperandOwner)
                .map(|(index, _)| *index)
                .unwrap();
            let nested_call = on_line
                .iter()
                .find(|(index, inst)| *index > push && {
                    matches!(inst.op, Op::Call | Op::ExprCall)
                })
                .map(|(index, _)| *index)
                .unwrap();
            let pop = on_line
                .iter()
                .find(|(index, inst)| *index > nested_call && inst.op == Op::PopCallOperandOwner)
                .map(|(index, _)| *index)
                .unwrap();
            assert!(push < nested_call && nested_call < pop, "{target}: line {line}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A may-alias string slice becomes independent before its transferred source pin retires.
#[test]
fn substr_of_an_evaluation_pin_is_persisted_before_source_retirement_on_all_targets() {
    use crate::ir::{Immediate, Op, Ownership, RuntimeCallTarget, RuntimeFnId};

    let source = r#"<?php
function sliceLength(): int { return 3; }
function sliceEvaluationOwner(string $source): string {
    return substr($source, 1, sliceLength());
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "sliceEvaluationOwner")
            .unwrap();
        let (call_index, call) = function
            .instructions
            .iter()
            .enumerate()
            .find(|(_, inst)| {
                matches!(
                    inst.immediate.as_ref(),
                    Some(Immediate::RuntimeCall(
                        RuntimeCallTarget::Function(RuntimeFnId::Substr)
                            | RuntimeCallTarget::ProfiledFunction {
                                target: RuntimeFnId::Substr,
                                ..
                            },
                    ))
                )
            })
            .expect("typed substr runtime call");
        let raw_slice = call.result.expect("substr result");
        let source_pin = call.operands[0];
        assert!(function.instructions.iter().any(|inst| {
            inst.op == Op::Acquire
                && inst.result == Some(source_pin)
                && inst.immediate == Some(Immediate::Bool(true))
        }), "{target}: substr source must be the marked evaluation pin");

        let (persist_index, persisted) = function
            .instructions
            .iter()
            .enumerate()
            .find(|(_, inst)| inst.op == Op::StrPersist && inst.operands == [raw_slice])
            .map(|(index, inst)| (index, inst.result.expect("persisted result")))
            .expect("substr evaluation-pin result must be persisted");
        assert_eq!(
            function.value(persisted).unwrap().ownership,
            Ownership::Owned,
            "{target}",
        );
        let releases = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.op == Op::Release && inst.operands == [source_pin])
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(releases.len(), 1, "{target}: source pin must retire exactly once");
        assert!(
            call_index < persist_index && persist_index < releases[0],
            "{target}: the slice must stabilize before its source pin retires",
        );
        assert!(!function.instructions.iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [raw_slice]
        }), "{target}: borrowed interior slice must not be released directly");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
