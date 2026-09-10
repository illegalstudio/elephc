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

/// Verifies one evaluation slot lends a borrow, then transfers and retires its lease once.
fn assert_evaluation_owner_transfers_once(
    function: &crate::ir::Function,
    borrowed: crate::ir::ValueId,
    slot: crate::ir::LocalSlotId,
    target: &str,
) {
    use crate::ir::{Immediate, Op, Ownership};

    let stores = function
        .instructions
        .iter()
        .filter(|inst| {
            inst.op == Op::StoreLocal
                && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .collect::<Vec<_>>();
    assert_eq!(stores.len(), 1, "{target}: evaluation owner slot must be stored once");
    let stored = stores[0].operands[0];
    assert!(function.instructions.iter().any(|inst| {
        inst.op == Op::Acquire && inst.result == Some(stored)
    }), "{target}: evaluation owner slot must store an acquired lease");

    for op in [Op::PushCallOperandOwner, Op::PopCallOperandOwner, Op::UnsetLocal] {
        assert_eq!(function.instructions.iter().filter(|inst| {
            inst.op == op && inst.immediate == Some(Immediate::LocalSlot(slot))
        }).count(), 1, "{target}: evaluation owner slot must contain one {op:?}");
    }
    assert!(!function.instructions.iter().any(|inst| {
        inst.op == Op::ReleaseLocalSlot
            && inst.immediate == Some(Immediate::LocalSlot(slot))
    }), "{target}: taking the evaluation owner must not release its cleared slot");

    let transferred = function
        .instructions
        .iter()
        .filter_map(|inst| {
            let value = inst.result?;
            (inst.op == Op::LoadLocal
                && inst.immediate == Some(Immediate::LocalSlot(slot))
                && value != borrowed
                && function
                    .value(value)
                    .is_some_and(|value| value.ownership != Ownership::Borrowed))
            .then_some(value)
        })
        .collect::<Vec<_>>();
    assert_eq!(transferred.len(), 1, "{target}: evaluation owner must be taken once");
    assert_eq!(function.instructions.iter().filter(|inst| {
        inst.op == Op::Release && inst.operands == [transferred[0]]
    }).count(), 1, "{target}: transferred evaluation lease must retire once");
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
function evaluationCallback(): int { return 1; }
class MixedArgumentSource {
    public mixed $text = "owned";
}
function takeNamedString(int $later, string $text): void {}
function takeNamedCallable(int $later, callable $callback): void {}
function exerciseMixedNamed(MixedArgumentSource $source, int $choice): void {
    $callback = $choice > 0 ? evaluationCallback(...) : evaluationCallback(...);
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
        let borrowed_evaluation_loads = function
            .instructions
            .iter()
            .filter_map(|inst| {
                let value = inst.result?;
                let Immediate::LocalSlot(slot) = inst.immediate.as_ref()? else {
                    return None;
                };
                (inst.op == Op::LoadLocal
                    && function.locals[(*slot).as_raw() as usize].kind == LocalKind::OwnedTemp
                    && function.value(value).is_some_and(|value| value.ownership == Ownership::Borrowed))
                .then_some((value, *slot))
            })
            .collect::<Vec<_>>();
        assert!(borrowed_evaluation_loads.len() >= 2, "{target}");
        for (borrowed, slot) in borrowed_evaluation_loads {
            assert!(!function.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [borrowed]
            }), "{target}: a borrowed evaluation-slot load must not be released");
            assert_evaluation_owner_transfers_once(function, borrowed, slot, target);
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
function acceptOwnedSpread(string $text, int $later): void {}
function exerciseOwnedSpread(string $text): void {
    acceptOwnedSpread(...createOwnedSpread($text), later: 1);
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
            let Immediate::LocalSlot(slot) = inst.immediate.as_ref()? else {
                return None;
            };
            (inst.op == Op::LoadLocal
                && function.locals[(*slot).as_raw() as usize].kind == LocalKind::OwnedTemp
                && function.value(value).is_some_and(|value| {
                    value.ownership == Ownership::Borrowed
                        && value.php_type.codegen_repr().is_php_array()
                }))
            .then_some((value, *slot))
        }).expect("spread evaluation must expose a borrowed owner-slot load");
        assert!(!function.instructions.iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [borrowed_spread.0]
        }), "{target}: spread staging must not release the borrowed ledger value");
        assert_evaluation_owner_transfers_once(
            function,
            borrowed_spread.0,
            borrowed_spread.1,
            target,
        );
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
