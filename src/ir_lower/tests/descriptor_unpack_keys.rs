//! Purpose:
//! Pins descriptor-invoker argument unpacking: boxed sources are walked with the runtime
//! iterator, and every key guard sits in its own basic block ahead of the write it protects.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - A declared PHP `array` is `Union([Array(Mixed), AssocArray])`, whose `codegen_repr()` is
//!   `Mixed`. `crate::ir::validator` rejects a `Heap(Mixed)` receiver on `Op::ArrayLen` and
//!   `Op::ArrayGet`, which is the exact CI failure these tests pin.
//! - Assertions are CFG aware. Guard ordering is proven by which block an instruction lives in
//!   and which block a terminator branches to, never by replaying the flat instruction list
//!   across branches.
//! - Lowering runs `validate_module`, so a reachable storage mismatch panics before assertions.
//! - Descriptor containers write through `Op::DescriptorArgSet` and probe through
//!   `Op::DescriptorArgKeyExists`, never `Op::HashSet` / `array_key_exists`: those normalize a
//!   numeric-string key into an integer, which for a descriptor container silently converts the
//!   NAME `$12` into position 12.

use crate::codegen::platform::Target;
use crate::ir::{BasicBlock, Function, Immediate, IrHeapKind, IrType, LocalKind, LocalSlotId, Op, Terminator};
use std::collections::HashMap;
use std::path::Path;

/// The five first-class targets descriptor unpacking must agree on.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Named and unpacked descriptor calls whose sources are declared PHP arrays.
const SOURCE: &str = r#"<?php
class UnpackAdder {
    public function add(int $first, int $second): int { return $first * 10 + $second; }
}
function unpackLeadingBoxed(): array { return [5 => 1]; }
function unpackNamedBoxed(): array { return ['second' => 2]; }
function addWithBoxedSpreadAndName(callable $callback): mixed {
    return call_user_func($callback, ...unpackLeadingBoxed(), second: 2);
}
function addWithTwoBoxedSpreads(callable $callback): mixed {
    return call_user_func($callback, ...unpackLeadingBoxed(), ...unpackNamedBoxed());
}
function addThroughDescriptorValue(callable $callback): mixed {
    return $callback(...unpackLeadingBoxed(), second: 2);
}
$callback = [new UnpackAdder(), 'add'];
echo addWithBoxedSpreadAndName($callback), addWithTwoBoxedSpreads($callback), addThroughDescriptorValue($callback);
"#;

/// Lowers `SOURCE` for `target` and hands back the named function plus the whole module.
fn lower_unpack_function(target: &str, name: &str) -> (crate::ir::Module, Function) {
    let module = super::lower_source_at_for_target(
        SOURCE,
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

/// Returns every block whose name matches exactly.
fn blocks_named<'a>(function: &'a Function, name: &str) -> Vec<&'a BasicBlock> {
    function.blocks.iter().filter(|block| block.name == name).collect()
}

/// Returns the ops of one block in its own order, resolved through the function's table.
fn block_ops(function: &Function, block: &BasicBlock) -> Vec<Op> {
    block
        .instructions
        .iter()
        .map(|inst| function.instructions[inst.as_raw() as usize].op)
        .collect()
}

/// Returns the name of a block a terminator's false edge reaches.
fn else_block_name<'a>(function: &'a Function, block: &BasicBlock) -> Option<&'a str> {
    match block.terminator.as_ref()? {
        Terminator::CondBr { else_target, .. } => {
            Some(function.blocks[else_target.as_raw() as usize].name.as_str())
        }
        _ => None,
    }
}

/// A boxed unpack source is never read through an indexed array header on any target.
///
/// This is the failing EIR validation in CI stated as a structural property: an `Op::ArrayLen`
/// or `Op::ArrayGet` whose receiver is `Heap(Mixed)` is exactly what the validator rejects.
#[test]
fn boxed_descriptor_unpack_never_reads_an_indexed_header_on_every_target() {
    for target in TARGETS {
        let module = super::lower_source_at_for_target(
            SOURCE,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(target).unwrap(),
        );
        for function in &module.functions {
            crate::ir::validate_function(function)
                .unwrap_or_else(|error| panic!("{target}: {} {error:?}", function.name));
            for inst in &function.instructions {
                if !matches!(inst.op, Op::ArrayLen | Op::ArrayGet | Op::ArrayGetSilent) {
                    continue;
                }
                let receiver = inst.operands[0];
                let ir_type = function.value(receiver).expect("operand is defined").ir_type;
                assert_ne!(
                    ir_type,
                    IrType::Heap(IrHeapKind::Mixed),
                    "{target}: {} reads a boxed cell as an indexed array header",
                    function.name,
                );
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Every unpack guard occupies its own block and branches to a throwing block.
///
/// The duplicate-name probe must be the LAST thing in its block: because that block ends in the
/// guard's `CondBr`, no `Op::DescriptorArgSet` can precede the rejection on any path through it.
#[test]
fn boxed_descriptor_unpack_guards_precede_every_write_on_every_target() {
    for target in TARGETS {
        for name in [
            "addWithBoxedSpreadAndName",
            "addWithTwoBoxedSpreads",
            "addThroughDescriptorValue",
        ] {
            let (_, function) = lower_unpack_function(target, name);
            let iterated = function
                .instructions
                .iter()
                .filter(|inst| inst.op == Op::IterStart)
                .count();
            assert!(iterated >= 1, "{target}/{name}: the unpack walks a runtime iterator");
            let named = blocks_named(&function, "descriptor.unpack.key.named");
            assert!(!named.is_empty(), "{target}/{name}: string keys get their own block");
            for block in named {
                let ops = block_ops(&function, block);
                assert!(
                    ops.contains(&Op::DescriptorArgKeyExists),
                    "{target}/{name}: the duplicate-name probe runs on the named key path",
                );
                assert!(
                    !ops.contains(&Op::DescriptorArgSet),
                    "{target}/{name}: nothing is written before the duplicate guard decides",
                );
                assert_eq!(
                    else_block_name(&function, block),
                    Some("descriptor.unpack.guard.throw"),
                    "{target}/{name}: a rejected name reaches the throwing block",
                );
            }
            for block in blocks_named(&function, "descriptor.unpack.guard.throw") {
                // A fixed-wording guard constructs its own `Error`. The duplicate-name guard
                // cannot: the offending name is a run-time key, so it calls the shared runtime
                // thrower and the block ends unreachable behind that call.
                let duplicate = block_ops(&function, block)
                    .contains(&Op::ThrowNamedParameterOverwrite);
                assert!(
                    match block.terminator {
                        Some(Terminator::Throw { .. }) => !duplicate,
                        Some(Terminator::Unreachable) => duplicate,
                        _ => false,
                    },
                    "{target}/{name}: an unpack guard rejection throws",
                );
            }
            assert!(
                function
                    .instructions
                    .iter()
                    .any(|inst| inst.op == Op::ThrowNamedParameterOverwrite),
                "{target}/{name}: the duplicate-name refusal names the offending key",
            );
            let unpackability = function.instructions.iter().any(|inst| {
                inst.op == Op::TypePredicate
                    && inst.immediate
                        == Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Iterable))
            });
            assert!(
                unpackability,
                "{target}/{name}: a non-iterable source is rejected before iteration",
            );
            assert!(
                blocks_named(&function, "descriptor.unpack.key.invalid")
                    .iter()
                    .all(|block| matches!(block.terminator, Some(Terminator::Throw { .. }))),
                "{target}/{name}: an unusable key throws instead of binding",
            );
        }
    }
}

/// Every spreading descriptor call builds exactly one container, with LIFO-clean owners.
///
/// `addWithTwoBoxedSpreads` belongs here as much as the mixed spread/name forms: two consecutive
/// unpack walks publish and retire two independent source/key/value owner groups into the same
/// container, which is the shape where a stale owner record would survive into the second walk.
#[test]
fn named_and_spread_descriptor_container_stays_single_on_every_target() {
    for target in TARGETS {
        for name in [
            "addWithBoxedSpreadAndName",
            "addWithTwoBoxedSpreads",
            "addThroughDescriptorValue",
        ] {
            let (_, function) = lower_unpack_function(target, name);
            assert_eq!(
                function.instructions.iter().filter(|inst| inst.op == Op::HashNew).count(),
                1,
                "{target}/{name}: one container carries the spread and the named argument",
            );
            assert_unpack_owners(&function, target);
        }
    }
}

/// Checks LIFO cleanup along CFG edges, including agreement across loop backedges.
fn assert_unpack_owners(function: &Function, target: &str) {
    let mut arrivals = HashMap::new();
    let mut pending = vec![(function.entry, Vec::<LocalSlotId>::new())];
    while let Some((id, mut owners)) = pending.pop() {
        if let Some(previous) = arrivals.get(&id) {
            assert_eq!(previous, &owners, "{target}: owner stacks disagree at {id:?}");
            continue;
        }
        arrivals.insert(id, owners.clone());
        let block = function.block(id).expect("reachable block exists");
        for instruction in &block.instructions {
            let instruction = function.instruction(*instruction).expect("block instruction exists");
            if instruction.op == Op::IterStart {
                match &instruction.immediate {
                    Some(Immediate::IterStart { owner: Some(slot), .. }) => {
                        assert_eq!(
                            owners.last(),
                            Some(slot),
                            "{target}: getIterator owner is the innermost LIFO record at IterStart in {}",
                            block.name
                        );
                    }
                    other => panic!(
                        "{target}: descriptor unpack IterStart must name an owner, got {other:?}"
                    ),
                }
            }
            if let Some(Immediate::LocalSlot(slot)) = &instruction.immediate {
                match instruction.op {
                    Op::PushCallOperandOwner => owners.push(*slot),
                    Op::PopCallOperandOwner => {
                        assert_eq!(owners.pop(), Some(*slot), "{target}: non-LIFO pop in {}", block.name);
                    }
                    Op::StoreLocal => {
                        if instruction.operands.first().is_some_and(|value| {
                            function.instructions.iter().any(|source| {
                                source.result == Some(*value)
                                    && matches!(source.op, Op::IterCurrentKey | Op::IterCurrentValue)
                            })
                        }) {
                            assert!(owners.contains(slot), "{target}: current iterator result needs an unwind root");
                            assert_eq!(function.locals[slot.as_raw() as usize].kind, LocalKind::OwnedTemp);
                        }
                    }
                    _ => {}
                }
            }
        }
        let successors = match block.terminator.as_ref().expect("terminated block") {
            Terminator::Br { target, .. } => vec![*target],
            Terminator::CondBr { then_target, else_target, .. } => vec![*then_target, *else_target],
            Terminator::Switch { cases, default, .. } => cases.iter().map(|case| case.target)
                .chain(std::iter::once(*default)).collect(),
            Terminator::Return { .. } => {
                assert!(owners.is_empty(), "{target}: normal return leaks owner records");
                Vec::new()
            }
            Terminator::Throw { .. } | Terminator::Fatal { .. } | Terminator::Unreachable => Vec::new(),
            Terminator::GeneratorSuspend { .. } => panic!("fixture has no generator"),
        };
        for successor in successors { pending.push((successor, owners.clone())); }
    }
    for iterator in function.instructions.iter().filter(|inst| inst.op == Op::IterStart) {
        let value = iterator.result.expect("iterator has a cursor result");
        assert!(!function.instructions.iter().any(|inst| {
            matches!(inst.op, Op::Release | Op::StoreLocal | Op::Acquire) && inst.operands.contains(&value)
        }), "{target}: the stack cursor is not a heap owner");
    }
}

/// Descriptor writes use the raw-key ops and always reload the container from its owner slot.
///
/// The receiver matters as much as the op. `__rt_hash_set` grows and REALLOCATES the table, and
/// the codegen write-back republishes the returned pointer into the slot the receiver was read
/// from. A write against anything other than the published construction slot would leave the
/// grown table unreachable from the next insertion and from exception cleanup.
#[test]
fn descriptor_unpack_writes_reach_their_published_owner_slot_on_every_target() {
    for target in TARGETS {
        for name in [
            "addWithBoxedSpreadAndName",
            "addWithTwoBoxedSpreads",
            "addThroughDescriptorValue",
        ] {
            let (_, function) = lower_unpack_function(target, name);
            let published: Vec<Immediate> = function
                .instructions
                .iter()
                .filter(|inst| inst.op == Op::PushCallOperandOwner)
                .filter_map(|inst| inst.immediate.clone())
                .collect();
            let writes: Vec<_> = function
                .instructions
                .iter()
                .filter(|inst| inst.op == Op::DescriptorArgSet)
                .collect();
            assert!(!writes.is_empty(), "{target}/{name}: the container takes raw-key writes");
            for write in writes {
                let receiver = write.operands[0];
                let load = function
                    .instructions
                    .iter()
                    .find(|inst| inst.result == Some(receiver))
                    .expect("a descriptor write borrows a defined receiver");
                assert_eq!(
                    load.op,
                    Op::LoadLocal,
                    "{target}/{name}: the container is reloaded, never carried across a growth",
                );
                let slot = load.immediate.clone().expect("a local load names its slot");
                assert!(
                    published.contains(&slot),
                    "{target}/{name}: a grown container must be republished into its owner slot",
                );
            }
            assert!(
                function
                    .instructions
                    .iter()
                    .any(|inst| inst.op == Op::DescriptorArgKeyExists),
                "{target}/{name}: the duplicate-name probe reads the raw descriptor key space",
            );
            for inst in function.instructions.iter().filter(|inst| inst.op == Op::HashSet) {
                let receiver = inst.operands[0];
                let Some(load) = function.instructions.iter().find(|i| i.result == Some(receiver))
                else { continue };
                let Some(slot) = load.immediate.clone() else { continue };
                assert!(
                    !published.contains(&slot),
                    "{target}/{name}: a descriptor container never takes a key-normalizing write",
                );
            }
        }
    }
}
