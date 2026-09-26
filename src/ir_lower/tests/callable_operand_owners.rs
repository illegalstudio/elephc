//! Purpose:
//! Pins the owner bookkeeping of callable operands: statically resolved extern and builtin
//! callables, descriptor callbacks published before their arguments, immediately invoked closure
//! descriptors, statically lowered `array_map()` results, and by-value reference-call leases.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Every published owner record is retired in strict LIFO order, because the backend reserves
//!   temporary stack for each record and releases it on the matching detach.
//! - A callable invoked through `call_user_func()` must get the same argument cleanup as the
//!   identical direct call.
//! - A call's own owned result is staged in a record published OUTSIDE every operand root and
//!   popped last, and it is CLEARED rather than released when it transfers to its consumer.
//! - A mixed named/spread `call_user_func()` builds one container instead of declining after the
//!   callback has already been evaluated and published.
//! - Every supported target lowers these paths to the same owner structure.

use crate::ir::{Function, Immediate, LocalKind, Op, Terminator};
use std::collections::HashMap;
use std::path::Path;

use crate::codegen::platform::Target;
use crate::types::PhpType;

/// A returned local widens its storage before promotion without boxing an obsolete load.
#[test]
fn reference_promotion_widens_the_local_without_reboxing_on_every_target() {
    let source = r#"<?php
class PromotionPayload { public function __destruct() { echo 'payload|'; } }
function &promoteReturnedLocal(array $marker): array {
    $values = [new PromotionPayload()];
    return $values;
}
function consumePromotion(): void { $alias = &promoteReturnedLocal([]); unset($alias); }
consumePromotion();
"#;
    for target in TARGETS {
        let (module, function) = lower_function(source, target, "promoteReturnedLocal");
        let (promotion, slot) = function.instructions.iter().enumerate().find_map(|(index, inst)| {
            let Some(Immediate::LocalSlotPair { first, .. }) = inst.immediate else { return None; };
            (inst.op == Op::PromoteLocalRefCell).then_some((index, first))
        }).expect("the returned local is promoted");
        assert_eq!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(), PhpType::Mixed,
            "{target}: the promoted slot has boxed storage");
        assert!(function.instructions[..promotion].iter().any(|inst|
            inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot))),
            "{target}: the concrete value reaches the widened slot before promotion");
        assert!(!function.instructions[..promotion].iter().any(|inst| {
            if inst.op != Op::MixedBox { return false; }
            let Some(&source) = inst.operands.first() else { return false; };
            function.instructions.iter().any(|load|
                load.result == Some(source) && load.op == Op::LoadLocal
                    && load.immediate == Some(Immediate::LocalSlot(slot)))
        }), "{target}: promotion must not rebox a concrete load from the same slot");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A by-value reference return clones the mutable Mixed pointee before retiring its lease.
#[test]
fn copied_reference_return_clones_mixed_pointees_on_every_target() {
    let source = r#"<?php
class MixedPayload {
    public mixed $value = [1, 'two'];
    public function &current(): mixed { return $this->value; }
}
function copyMixedPayload(MixedPayload $source): mixed { return $source->current(); }
echo count(copyMixedPayload(new MixedPayload()));
"#;
    for target in TARGETS {
        let (module, caller) = lower_function(source, target, "copyMixedPayload");
        let cloned_reference = caller.instructions.iter().any(|instruction| {
            instruction.op == Op::MixedClone
                && instruction.operands.first().is_some_and(|value| {
                    caller.instructions.iter().any(|source| {
                        source.result == Some(*value) && source.op == Op::LoadRefCell
                    })
                })
        });
        assert!(cloned_reference, "{target}: a reference copy must detach its Mixed box");
        assert_owner_records_are_lifo(&caller, target);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// The five first-class targets every callable-ownership lowering path must agree on.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Lowers `source` for `target` and hands back the named function plus the whole module.
fn lower_function(source: &str, target: &str, name: &str) -> (crate::ir::Module, Function) {
    let module = super::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::parse(target).unwrap(),
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{target}: {name} is lowered"))
        .clone();
    (module, function)
}

/// Proves every owner record in `function` nests and retires in strict publication order.
///
/// Owner retirement must be followed through the CFG. A descriptor unpack guard has distinct
/// invalid-key and normal-exit cleanup blocks, so replaying the flat instruction table would
/// incorrectly treat both mutually exclusive pops as one execution path.
fn assert_owner_records_are_lifo(function: &Function, target: &str) {
    let mut arrivals = HashMap::new();
    let mut pending = vec![(function.entry, Vec::<crate::ir::LocalSlotId>::new())];
    while let Some((block_id, mut active)) = pending.pop() {
        if let Some(previous) = arrivals.get(&block_id) {
            assert_eq!(
                previous, &active,
                "{target}: {} owner stacks agree at {block_id:?}",
                function.name,
            );
            continue;
        }
        arrivals.insert(block_id, active.clone());
        let block = function.block(block_id).expect("reachable block exists");
        for instruction in &block.instructions {
            let inst = function
                .instruction(*instruction)
                .expect("block instruction exists");
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                continue;
            };
            match inst.op {
                Op::PushCallOperandOwner => active.push(slot),
                Op::PopCallOperandOwner => {
                    assert_eq!(
                        active.pop(),
                        Some(slot),
                        "{target}: {} retires its owner records in LIFO order in {}",
                        function.name,
                        block.name,
                    );
                }
                _ => {}
            }
        }
        let successors = match block.terminator.as_ref().expect("terminated block") {
            Terminator::Br { target, .. } => vec![*target],
            Terminator::CondBr {
                then_target,
                else_target,
                ..
            } => vec![*then_target, *else_target],
            Terminator::Switch { cases, default, .. } => cases
                .iter()
                .map(|case| case.target)
                .chain(std::iter::once(*default))
                .collect(),
            Terminator::GeneratorSuspend { resume, .. } => vec![*resume],
            Terminator::Return { .. } => {
                assert!(
                    active.is_empty(),
                    "{target}: {} retires every owner record it publishes before returning",
                    function.name,
                );
                Vec::new()
            }
            Terminator::Throw { .. } | Terminator::Fatal { .. } | Terminator::Unreachable => {
                Vec::new()
            }
        };
        for successor in successors {
            pending.push((successor, active.clone()));
        }
    }
}

/// Returns the index of the first instruction with `op`.
fn first(function: &Function, op: Op) -> Option<usize> {
    function.instructions.iter().position(|inst| inst.op == op)
}

/// Returns the frame slot a rooted operand was stored into, following its acquire.
fn rooted_operand_slot(function: &Function, operand: crate::ir::ValueId) -> crate::ir::LocalSlotId {
    let defining = function
        .instructions
        .iter()
        .find(|inst| inst.result == Some(operand))
        .expect("the operand has a defining instruction");
    assert_eq!(
        defining.op,
        Op::Acquire,
        "the operand is the lease a published root acquired, not the raw temporary",
    );
    function
        .instructions
        .iter()
        .find_map(|inst| match inst.immediate {
            Some(Immediate::LocalSlot(slot))
                if inst.op == Op::StoreLocal && inst.operands == [operand] =>
            {
                Some(slot)
            }
            _ => None,
        })
        .expect("the acquired lease is stored into its owner slot")
}

/// Returns the index of the instruction carrying `op` and `slot`.
fn slot_instruction(function: &Function, op: Op, slot: crate::ir::LocalSlotId) -> Option<usize> {
    function.instructions.iter().position(|inst| {
        inst.op == op && inst.immediate == Some(Immediate::LocalSlot(slot))
    })
}

/// A statically resolved extern callable releases its argument temporaries like the direct call.
///
/// `ExternCall` consumes its operands; an owned string built for the call is not handed back by
/// the C ABI, so the callable form has to release it exactly where the direct form does.
#[test]
fn static_extern_callable_releases_arguments_like_the_direct_call_on_every_target() {
    let source = r#"<?php
extern function atoi(string $value): int;
function parseDirectly(string $left, string $right): int {
    return atoi(implode('', [$left, $right]));
}
function parseThroughCallable(string $left, string $right): int {
    return call_user_func('atoi', implode('', [$left, $right]));
}
echo parseDirectly('4', '2'), parseThroughCallable('4', '2');
"#;
    for name in TARGETS {
        let (module, direct) = lower_function(source, name, "parseDirectly");
        let callable = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("parseThroughCallable"))
            .expect("the callable form is lowered");
        let releases_after_call = |function: &Function| {
            let call = first(function, Op::ExternCall).expect("the extern call is lowered");
            function
                .instructions
                .iter()
                .enumerate()
                .filter(|(index, inst)| *index > call && inst.op == Op::Release)
                .count()
        };
        let direct_releases = releases_after_call(&direct);
        assert!(
            direct_releases > 0,
            "{name}: the direct extern call releases its owned string argument",
        );
        assert_eq!(
            releases_after_call(callable),
            direct_releases,
            "{name}: the callable form releases the same argument temporaries",
        );
        assert_owner_records_are_lifo(callable, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A statically resolved builtin callable publishes earlier arguments like the direct call.
///
/// Source-order evaluation roots each owned argument before the next one is evaluated, so a
/// later argument that throws cannot strand an earlier one. The callable form must run the same
/// ledger as the identical direct call.
#[test]
fn static_builtin_callable_publishes_argument_owners_like_the_direct_call_on_every_target() {
    let source = r#"<?php
function replaceDirectly(string $left, string $right): string {
    return str_replace(implode('', [$left, $right]), '-', 'a-b');
}
function replaceThroughCallable(string $left, string $right): string {
    return call_user_func('str_replace', implode('', [$left, $right]), '-', 'a-b');
}
echo replaceDirectly('a', 'b'), replaceThroughCallable('a', 'b');
"#;
    for name in TARGETS {
        let (module, direct) = lower_function(source, name, "replaceDirectly");
        let callable = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("replaceThroughCallable"))
            .expect("the callable form is lowered");
        let published = |function: &Function| {
            function
                .instructions
                .iter()
                .filter(|inst| inst.op == Op::PushCallOperandOwner)
                .count()
        };
        let direct_published = published(&direct);
        assert!(
            direct_published > 0,
            "{name}: the direct builtin call publishes its evaluated argument owner",
        );
        assert_eq!(
            published(callable),
            direct_published,
            "{name}: the callable form publishes the same argument owners",
        );
        for function in [&direct, callable] {
            let (call_index, call) = function.instructions.iter().enumerate().find(|(_, inst)| {
                matches!(
                    inst.immediate,
                    Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
                        crate::ir::RuntimeFnId::StrReplace,
                    ))),
                )
            }).expect("string replacement uses its typed runtime target");
            let search = call.operands[0];
            assert!(
                function.instructions[call_index + 1..].iter().any(|inst| {
                    inst.op == Op::Release && inst.operands == [search]
                }),
                "{name}: {} retires its owned search after copying the result", function.name,
            );
        }
        assert_owner_records_are_lifo(callable, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A descriptor callback is published before the first argument expression is lowered.
///
/// The callback here is a freshly built callable array holding a new object. Nothing else owns
/// it while the argument container is being built, and building that container runs PHP code
/// that can throw.
#[test]
fn descriptor_callback_is_published_before_its_arguments_on_every_target() {
    let source = r#"<?php
class PublishedHandler {
    public function handle(int $value): int { return $value; }
}
function invokeFreshCallable(int $count): int {
    return call_user_func([new PublishedHandler(), 'handle'], $count + 1);
}
echo invokeFreshCallable($argc);
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "invokeFreshCallable");
        let invoke = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::CallableDescriptorInvoke)
            .expect("the descriptor invocation is lowered");
        let slot = rooted_operand_slot(&caller, invoke.operands[0]);
        let publish = slot_instruction(&caller, Op::PushCallOperandOwner, slot)
            .expect("a cleanup record covers the callback lease");
        let detach = slot_instruction(&caller, Op::PopCallOperandOwner, slot)
            .expect("the callback record is detached, not leaked");
        // Two arrays are allocated here: the callable array that is the callback, and the
        // descriptor invoker's argument container. The callback lease has to be published
        // between them, because filling the container runs the argument expressions.
        let allocations = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.op == Op::ArrayNew)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            allocations.len(),
            2,
            "{name}: the callable array and the argument container are both allocated",
        );
        assert!(
            allocations[0] < publish && publish < allocations[1] && allocations[1] < detach,
            "{name}: allocations {allocations:?} around publish {publish} and detach {detach}",
        );
        assert!(
            slot_instruction(&caller, Op::ReleaseLocalSlot, slot).is_some(),
            "{name}: the callback lease is released after the invocation",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// An immediately invoked closure literal retires the descriptor its direct call does not use.
///
/// The direct call passes the captured values, not the descriptor, but the descriptor owns the
/// captured environment until the call returns, so it is published for the call and retired
/// afterwards instead of being dropped on the floor.
#[test]
fn immediately_invoked_closure_retires_its_descriptor_on_every_target() {
    let source = r#"<?php
function invokeClosureLiteral(int $count): int {
    $items = [$count, $count + 1];
    return (function () use ($items): int { return count($items); })();
}
echo invokeClosureLiteral($argc);
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "invokeClosureLiteral");
        let descriptor = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::ClosureNew)
            .and_then(|inst| inst.result)
            .expect("the closure literal is lowered");
        let lease = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::Acquire && inst.operands == [descriptor])
            .and_then(|inst| inst.result)
            .expect("the descriptor is acquired into a published lease");
        let slot = rooted_operand_slot(&caller, lease);
        assert_eq!(
            caller.locals[slot.as_raw() as usize].kind,
            LocalKind::HiddenTemp,
            "{name}: the descriptor lease lives in a hidden operand slot",
        );
        let publish = slot_instruction(&caller, Op::PushCallOperandOwner, slot)
            .expect("a cleanup record covers the descriptor lease");
        let call = first(&caller, Op::Call).expect("the closure body is called directly");
        let release = slot_instruction(&caller, Op::ReleaseLocalSlot, slot)
            .expect("the unused descriptor is released");
        assert!(
            publish < call && call < release,
            "{name}: publish {publish} < call {call} < release {release}",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// The statically lowered `array_map()` result is published and reloaded across every callback.
///
/// The partially built result owns every element already mapped, and each callback can throw. It
/// is also reallocated by growth, so the current pointer has to be reloaded from the published
/// slot rather than kept in the value that `array_new` produced.
#[test]
fn static_array_map_publishes_its_partial_result_on_every_target() {
    let source = r#"<?php
function upperItem(string $text): string { return strtoupper($text); }
function mapLiteralItems(): array {
    return array_map('upperItem', ['a', 'b']);
}
echo implode(',', mapLiteralItems());
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "mapLiteralItems");
        let result = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::ArrayNew)
            .and_then(|inst| inst.result)
            .expect("the result array is allocated");
        let lease = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::Acquire && inst.operands == [result])
            .and_then(|inst| inst.result)
            .expect("the result array is acquired into a published lease");
        let slot = rooted_operand_slot(&caller, lease);
        let publish = slot_instruction(&caller, Op::PushCallOperandOwner, slot)
            .expect("a cleanup record covers the partial result");
        let calls = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.op == Op::Call)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2, "{name}: one callback call per mapped element");
        let release = slot_instruction(&caller, Op::ReleaseLocalSlot, slot)
            .expect("the construction slot is retired once the result transfers out");
        assert!(
            publish < calls[0] && calls[1] < release,
            "{name}: publish {publish} < calls {calls:?} < release {release}",
        );
        let reloads = caller
            .instructions
            .iter()
            .filter(|inst| {
                inst.op == Op::LoadLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
            })
            .count();
        assert_eq!(
            reloads, 3,
            "{name}: the current pointer is reloaded per push and once to transfer the result",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// An ordinary by-value use of a reference-returning call publishes its lease before arguments.
///
/// The copy is taken only after the caller's own argument cleanup can no longer throw, so the
/// lease is the sole owner of the pointee while those destructors run, and its record nests
/// outside every argument root the call published.
#[test]
fn by_value_reference_return_publishes_its_lease_before_arguments_on_every_target() {
    let source = r#"<?php
class ValueLeaseHolder {
    public array $items = [1];
}
function &valueLease(array $extra, ValueLeaseHolder $holder): array {
    return $holder->items;
}
function copyValueLease(int $size): void {
    $holder = new ValueLeaseHolder();
    $copy = valueLease(array_fill(0, $size, 'a'), $holder);
    echo count($copy);
}
copyValueLease($argc);
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "copyValueLease");
        let (adopt, adopted) = caller
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| match inst.immediate {
                Some(Immediate::LocalSlotPair { second, .. }) if inst.op == Op::AdoptRefCellPtr => {
                    Some((index, second))
                }
                _ => None,
            })
            .expect("the transferred cell is adopted into a hidden owner slot");
        assert_eq!(
            caller.locals[adopted.as_raw() as usize].kind,
            LocalKind::RefCell,
            "{name}: the by-value lease lives in a reference-cell owner slot",
        );
        let publish = slot_instruction(&caller, Op::PushCallOperandOwner, adopted)
            .expect("a cleanup record covers the by-value lease");
        let detach = slot_instruction(&caller, Op::PopCallOperandOwner, adopted)
            .expect("the lease record is detached, not leaked");
        let call = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(index, inst)| inst.op == Op::Call && *index < adopt)
            .map(|(index, _)| index)
            .next_back()
            .expect("the reference-returning call is lowered before its cell is adopted");
        assert!(
            publish < call && call < adopt && adopt < detach,
            "{name}: publish {publish} < call {call} < adopt {adopt} < detach {detach}",
        );
        // The lease is published before the argument roots, so its record is the outermost one
        // and nothing may retire it until they have all retired.
        let argument_detaches = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                inst.op == Op::PopCallOperandOwner
                    && inst.immediate != Some(Immediate::LocalSlot(adopted))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert!(
            argument_detaches.iter().all(|index| *index < detach),
            "{name}: argument records {argument_detaches:?} retire before the lease at {detach}",
        );
        let staged = match caller.instructions[adopt].immediate {
            Some(Immediate::LocalSlotPair { first, .. }) => first,
            _ => panic!("{name}: the adoption names its staging local"),
        };
        let dereference = caller
            .instructions
            .iter()
            .position(|inst| {
                inst.op == Op::LoadRefCell && inst.immediate == Some(Immediate::LocalSlot(staged))
            })
            .expect("the payload is read back out of the lease");
        assert!(
            argument_detaches.iter().all(|index| *index < dereference),
            "{name}: the copy is taken after argument cleanup at {dereference}",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Returns the frame slot an owned call result was moved into right after the call.
fn staged_result_slot(function: &Function, result: crate::ir::ValueId) -> crate::ir::LocalSlotId {
    function
        .instructions
        .iter()
        .find_map(|inst| match inst.immediate {
            Some(Immediate::LocalSlot(slot))
                if inst.op == Op::StoreLocal && inst.operands == [result] =>
            {
                Some(slot)
            }
            _ => None,
        })
        .expect("the owned call result is moved into its published staging slot")
}

/// Returns the index of the last instruction carrying `op` and `slot`.
fn last_slot_instruction(
    function: &Function,
    op: Op,
    slot: crate::ir::LocalSlotId,
) -> Option<usize> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.op == op && inst.immediate == Some(Immediate::LocalSlot(slot)))
        .map(|(index, _)| index)
        .next_back()
}

/// A descriptor invocation stages its owned result OUTSIDE the callback and container records.
///
/// Retiring the argument container and the callback runs PHP destructors that can throw into a
/// catch in this same frame, and the result is only an SSA temporary until the staging holds it.
/// The staging must therefore be published first and popped last, because the runtime's operand
/// scope is a plain LIFO stack that cannot detach an arbitrary named record.
#[test]
fn descriptor_invocation_stages_its_result_outside_every_operand_record_on_every_target() {
    let source = r#"<?php
class ResultHandler {
    public function handle(int $value): string { return 'v' . $value; }
}
function invokeForResult(int $count): string {
    return call_user_func([new ResultHandler(), 'handle'], $count + 1);
}
echo invokeForResult($argc);
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "invokeForResult");
        let invoke = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::CallableDescriptorInvoke)
            .expect("the descriptor invocation is lowered");
        let result = invoke.result.expect("the invocation produces a value");
        let result_slot = staged_result_slot(&caller, result);
        assert_eq!(
            caller.locals[result_slot.as_raw() as usize].kind,
            LocalKind::OwnedTemp,
            "{name}: the staged result lives in a one-shot owned temporary",
        );
        let callback_slot = rooted_operand_slot(&caller, invoke.operands[0]);
        let result_publish = slot_instruction(&caller, Op::PushCallOperandOwner, result_slot)
            .expect("a cleanup record covers the owned result");
        let callback_publish = slot_instruction(&caller, Op::PushCallOperandOwner, callback_slot)
            .expect("a cleanup record covers the callback lease");
        let result_detach = last_slot_instruction(&caller, Op::PopCallOperandOwner, result_slot)
            .expect("the result record is detached, not leaked");
        let callback_detach =
            last_slot_instruction(&caller, Op::PopCallOperandOwner, callback_slot)
                .expect("the callback record is detached, not leaked");
        assert!(
            result_publish < callback_publish && callback_detach < result_detach,
            "{name}: result record [{result_publish}, {result_detach}] encloses the callback \
             record [{callback_publish}, {callback_detach}]",
        );
        // The result transfers OUT of the staging slot: a `release_local_slot` here would free
        // the very value the expression hands to its consumer.
        assert!(
            slot_instruction(&caller, Op::ReleaseLocalSlot, result_slot).is_none(),
            "{name}: the staging slot is cleared, never released",
        );
        assert!(
            slot_instruction(&caller, Op::UnsetLocal, result_slot).is_some(),
            "{name}: the staging slot is zeroed once the result transfers out",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A mixed named and spread `call_user_func()` builds a container instead of declining.
///
/// The callback is published before the argument container is built, so there is no shape this
/// builder may refuse: declining would either strand the callback's owner record or make the
/// caller re-lower the callback expression and run its side effects twice. The spread merges into
/// the same published hash the named argument writes into.
#[test]
fn named_and_spread_call_user_func_builds_one_published_container_on_every_target() {
    let source = r#"<?php
class SpreadHandler {
    public function handle(int $first, int $second): int { return $first * 10 + $second; }
}
function invokeNamedSpread(array $leading): int {
    return call_user_func([new SpreadHandler(), 'handle'], ...$leading, second: 2);
}
echo invokeNamedSpread([$argc]);
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "invokeNamedSpread");
        assert!(
            first(&caller, Op::CallableDescriptorInvoke).is_some(),
            "{name}: the mixed named/spread shape reaches descriptor invocation",
        );
        let hash = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::HashNew)
            .and_then(|inst| inst.result)
            .expect("the named argument container is allocated");
        let lease = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::Acquire && inst.operands == [hash])
            .and_then(|inst| inst.result)
            .expect("the container is acquired into a published lease");
        let slot = rooted_operand_slot(&caller, lease);
        assert!(
            slot_instruction(&caller, Op::PushCallOperandOwner, slot).is_some(),
            "{name}: a cleanup record covers the partially built container",
        );
        // The spread writes runtime numeric keys into the same container the named argument
        // writes into, so exactly one container is built for the whole call.
        assert_eq!(
            caller.instructions.iter().filter(|inst| inst.op == Op::HashNew).count(),
            1,
            "{name}: one container carries both the spread and the named argument",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A direct user call stages its fresh owned result outside the argument roots it retires.
///
/// `root_user_call_operands` publishes a root per owned argument, and retiring those roots runs
/// PHP destructors. Without this staging the call's own result is unreachable from that unwind.
#[test]
fn direct_user_call_stages_its_owned_result_outside_argument_roots_on_every_target() {
    let source = r#"<?php
class FreshResult {
    public string $tag = '';
}
function makeFresh(string $tag): FreshResult {
    $result = new FreshResult();
    $result->tag = $tag;
    return $result;
}
function callWithFreshResult(string $left, string $right): FreshResult {
    return makeFresh(implode('', [$left, $right]));
}
echo callWithFreshResult('a', 'b')->tag;
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "callWithFreshResult");
        let call = caller
            .instructions
            .iter()
            .find(|inst| inst.op == Op::Call)
            .expect("the direct user call is lowered");
        let result = call.result.expect("the call produces a value");
        let result_slot = staged_result_slot(&caller, result);
        assert_eq!(
            caller.locals[result_slot.as_raw() as usize].kind,
            LocalKind::OwnedTemp,
            "{name}: the staged result lives in a one-shot owned temporary",
        );
        let result_publish = slot_instruction(&caller, Op::PushCallOperandOwner, result_slot)
            .expect("a cleanup record covers the owned result");
        let result_detach = last_slot_instruction(&caller, Op::PopCallOperandOwner, result_slot)
            .expect("the result record is detached, not leaked");
        let other_records = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                matches!(inst.op, Op::PushCallOperandOwner | Op::PopCallOperandOwner)
                    && inst.immediate != Some(Immediate::LocalSlot(result_slot))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert!(
            !other_records.is_empty(),
            "{name}: the owned string argument is rooted across the call",
        );
        assert!(
            other_records
                .iter()
                .all(|index| *index > result_publish && *index < result_detach),
            "{name}: argument records {other_records:?} nest inside the result record \
             [{result_publish}, {result_detach}]",
        );
        assert!(
            slot_instruction(&caller, Op::ReleaseLocalSlot, result_slot).is_none(),
            "{name}: the staging slot is cleared, never released",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A managed reference argument detaches its record before its release can rethrow.
///
/// The local reference-cell helper clears and retires the cell inside its own bounded cleanup
/// boundary. If its record remains linked during that release, a destructor throw can stop the
/// outer unwind before it reaches older argument records and the prepublished string result.
#[test]
fn managed_ref_argument_detaches_before_release_inside_result_staging_on_every_target() {
    let source = r#"<?php
class ManagedLeaseCleanupBomb {
    public function __destruct() { throw new Exception('cleanup'); }
}
function detachManagedLeaseParent(array &$items): mixed {
    $items = [];
    return null;
}
function invokeManagedLease(array $items): string {
    $callback = function (mixed &$value, mixed $unused): string {
        return str_repeat('r', 6);
    };
    return $callback($items['k'], detachManagedLeaseParent($items));
}
echo invokeManagedLease(['k' => new ManagedLeaseCleanupBomb()]);
"#;
    for name in TARGETS {
        let (module, caller) = lower_function(source, name, "invokeManagedLease");
        let (call_index, call) = caller
            .instructions
            .iter()
            .enumerate()
            .rev()
            .find(|(_, inst)| inst.op == Op::Call)
            .expect("the statically resolved closure call is lowered directly");
        let result = call.result.expect("the closure call produces a string");
        let result_slot = staged_result_slot(&caller, result);
        let result_publish = slot_instruction(&caller, Op::PushCallOperandOwner, result_slot)
            .expect("the string result has an unwind record");
        let result_detach = last_slot_instruction(&caller, Op::PopCallOperandOwner, result_slot)
            .expect("the string result record is detached on success");
        let cell_slot = caller
            .instructions
            .iter()
            .find_map(|inst| {
                let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                    return None;
                };
                (inst.op == Op::ReleaseLocalRefCell).then_some(slot)
            })
            .expect("the managed argument lease is retired");
        let cell_publish = slot_instruction(&caller, Op::PushCallOperandOwner, cell_slot)
            .expect("the managed argument lease has an unwind record");
        let cell_detach = last_slot_instruction(&caller, Op::PopCallOperandOwner, cell_slot)
            .expect("the managed argument record is detached on success");
        let cleanup_block = caller
            .blocks
            .iter()
            .find(|block| {
                block.instructions.iter().any(|instruction| {
                    caller.instruction(*instruction).is_some_and(|inst| {
                        inst.op == Op::ReleaseLocalRefCell
                            && inst.immediate == Some(Immediate::LocalSlot(cell_slot))
                    })
                })
            })
            .expect("the managed argument cleanup block exists");
        let cleanup_ops = cleanup_block
            .instructions
            .iter()
            .filter_map(|instruction| caller.instruction(*instruction))
            .filter_map(|inst| {
                (inst.immediate == Some(Immediate::LocalSlot(cell_slot))).then_some(inst.op)
            })
            .collect::<Vec<_>>();
        assert!(
            result_publish < cell_publish
                && cell_publish < call_index
                && call_index < cell_detach,
            "{name}: the result and managed argument records are published before the call",
        );
        // Result staging may lease and detach the same slot again while the call's value is
        // being published; what must hold is that the cell's LAST detach immediately precedes
        // its release, so the unwind record never outlives the cell it names.
        assert!(
            cleanup_ops.ends_with(&[Op::PopCallOperandOwner, Op::ReleaseLocalRefCell])
                && cleanup_ops
                    .iter()
                    .filter(|op| **op == Op::ReleaseLocalRefCell)
                    .count()
                    == 1,
            "{name}: the cleanup block detaches the managed argument before release, got {cleanup_ops:?}",
        );
        assert!(
            cell_detach < result_detach,
            "{name}: the result record encloses the managed argument record",
        );
        assert_owner_records_are_lifo(&caller, name);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
