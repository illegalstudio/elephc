//! Purpose:
//! Structural coverage for the Mixed owner that holds
//! `IteratorAggregate::getIterator()` results produced inside `Op::IterStart`.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Every supported target must publish an OwnedTemp Mixed slot before
//!   `IterStart`, populate that slot only in the backend, and retire it with
//!   `PopCallOperandOwner` then `ReleaseLocalSlot` on normal completion,
//!   skipped-loop `break`, `return`, and `throw`. Innermost `break` and
//!   `continue` must not double-retire.

use crate::codegen::platform::Target;
use crate::ir::{
    validate_function, Builder, Effects, Function, Immediate, IrHeapKind, IrType, LocalKind,
    LocalSlotId, Op, Ownership, Terminator,
};
use crate::types::PhpType;
use std::collections::HashMap;
use std::path::Path;

/// The five first-class targets this owner contract must agree on.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Shared IteratorAggregate and Iterator fixtures used by the structural cases.
const PRELUDE: &str = r#"
class RangeIter implements Iterator {
    private int $i = 0;
    private int $n;
    public function __construct(int $n) { $this->n = $n; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->n; }
}
class FreshAgg implements IteratorAggregate {
    public function getIterator(): Iterator { return new RangeIter(2); }
}
class SelfAgg implements Iterator, IteratorAggregate {
    private int $i = 0;
    public function getIterator(): Iterator { return $this; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
class ExistingAgg implements IteratorAggregate {
    private RangeIter $it;
    public function __construct() { $this->it = new RangeIter(2); }
    public function getIterator(): Iterator { return $this->it; }
}
"#;

/// Lowers `source` for `target` and returns the named function.
fn lower_function(target: &str, source: &str, name: &str) -> Function {
    let module = super::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::parse(target).unwrap(),
    );
    module
        .functions
        .iter()
        .find(|function| function.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{target}: {name} is lowered"))
        .clone()
}

/// Returns every IterStart index together with its optional owner slot.
fn iter_start_owners(function: &Function) -> Vec<(usize, Option<LocalSlotId>)> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.op == Op::IterStart)
        .map(|(index, inst)| {
            let owner = match inst.immediate.as_ref() {
                Some(Immediate::IterStart { owner, .. }) => *owner,
                other => panic!("IterStart must carry the structured immediate, got {other:?}"),
            };
            (index, owner)
        })
        .collect()
}

/// Returns the IterStart that owns a getIterator result.
fn owned_iter_start(function: &Function) -> (usize, LocalSlotId) {
    let owned = iter_start_owners(function)
        .into_iter()
        .filter_map(|(index, owner)| owner.map(|slot| (index, slot)))
        .collect::<Vec<_>>();
    match owned.as_slice() {
        [(index, slot)] => (*index, *slot),
        other => panic!("expected one owned IterStart, got {other:?}"),
    }
}

/// Returns true when `slot` is an OwnedTemp Mixed local in `function`.
fn slot_is_owned_mixed_temp(function: &Function, slot: LocalSlotId) -> bool {
    function
        .locals
        .get(slot.as_raw() as usize)
        .is_some_and(|local| {
            local.id == slot
                && local.kind == LocalKind::OwnedTemp
                && local.php_type.codegen_repr() == PhpType::Mixed
        })
}

/// Asserts the owner is a valid Mixed temp, published before IterStart, and
/// balanced on every CFG path. Continue and innermost break keep it live.
fn assert_single_owner_lifetime(function: &Function, target: &str) {
    let (start, owner) = owned_iter_start(function);
    assert!(
        slot_is_owned_mixed_temp(function, owner),
        "{target}: owner slot[{}] must be a valid OwnedTemp Mixed",
        owner.as_raw()
    );
    assert!(
        function.instructions[..start].iter().any(|inst| {
            inst.op == Op::PushCallOperandOwner && inst.immediate == Some(Immediate::LocalSlot(owner))
        }),
        "{target}: owner must be published before IterStart"
    );
    let mut arrivals = HashMap::new();
    let mut pending = vec![(function.entry, false)];
    while let Some((id, mut live)) = pending.pop() {
        if let Some(&previous) = arrivals.get(&id) {
            assert_eq!(previous, live, "{target}: owner liveness disagrees at {id:?}");
            continue;
        }
        arrivals.insert(id, live);
        let block = function.block(id).expect("reachable block exists");
        for inst_id in &block.instructions {
            let inst = function.instruction(*inst_id).expect("block instruction exists");
            if inst.immediate != Some(Immediate::LocalSlot(owner)) {
                continue;
            }
            match inst.op {
                Op::PushCallOperandOwner => {
                    assert!(!live, "{target}: owner pushed twice in {}", block.name);
                    live = true;
                }
                Op::PopCallOperandOwner => {
                    assert!(live, "{target}: owner popped while unset in {}", block.name);
                    live = false;
                }
                Op::ReleaseLocalSlot => {
                    assert!(
                        !live,
                        "{target}: owner released while still published in {}",
                        block.name
                    );
                }
                _ => {}
            }
        }
        match block.terminator.as_ref().expect("terminated block") {
            Terminator::Br { target: next, .. } => pending.push((*next, live)),
            Terminator::CondBr {
                then_target,
                else_target,
                ..
            } => {
                pending.push((*then_target, live));
                pending.push((*else_target, live));
            }
            Terminator::Switch { cases, default, .. } => {
                for case in cases {
                    pending.push((case.target, live));
                }
                pending.push((*default, live));
            }
            Terminator::Return { .. } | Terminator::Throw { .. } => {
                assert!(
                    !live,
                    "{target}: owner still published at {} terminator",
                    block.name
                );
            }
            Terminator::Fatal { .. } | Terminator::Unreachable => {}
            Terminator::GeneratorSuspend { .. } => panic!("{target}: fixture has no generator"),
        }
    }
}

/// Temporary and local aggregates both publish a valid owner on every target.
#[test]
fn temporary_and_local_aggregates_own_get_iterator_results_on_every_target() {
    let source = format!(
        "<?php {PRELUDE}
function walkTemp(): void {{ foreach (new FreshAgg() as $v) {{ echo $v; }} }}
function walkLocal(): void {{ $a = new FreshAgg(); foreach ($a as $v) {{ echo $v; }} }}
"
    );
    for target in TARGETS {
        assert_single_owner_lifetime(&lower_function(target, &source, "walkTemp"), target);
        assert_single_owner_lifetime(&lower_function(target, &source, "walkLocal"), target);
    }
}

/// Returning a fresh Iterator and returning `$this` both use the same owner contract.
#[test]
fn get_iterator_this_and_fresh_iterator_use_the_same_owner_on_every_target() {
    let source = format!(
        "<?php {PRELUDE}
function walkFresh(): void {{ foreach (new FreshAgg() as $v) {{ echo $v; }} }}
function walkExisting(): void {{ foreach (new ExistingAgg() as $v) {{ echo $v; }} }}
function walkSelf(): void {{ foreach (new SelfAgg() as $v) {{ echo $v; }} }}
"
    );
    for target in TARGETS {
        assert_single_owner_lifetime(&lower_function(target, &source, "walkFresh"), target);
        assert_single_owner_lifetime(&lower_function(target, &source, "walkExisting"), target);
        // SelfAgg implements Iterator, so foreach must not call getIterator.
        let function = lower_function(target, &source, "walkSelf");
        let owners = iter_start_owners(&function);
        assert!(
            owners.iter().all(|(_, owner)| owner.is_none()),
            "{target}: a direct Iterator source must not allocate a getIterator owner"
        );
    }
}

/// Normal completion, innermost break, continue, break 2, and return retire the owner once.
#[test]
fn loop_exits_retire_the_owner_without_double_release_on_every_target() {
    let source = format!(
        "<?php {PRELUDE}
function walkEnd(): void {{ foreach (new FreshAgg() as $v) {{ echo $v; }} }}
function walkBreak(): void {{ foreach (new FreshAgg() as $v) {{ echo $v; break; }} }}
function walkContinue(): void {{ foreach (new FreshAgg() as $v) {{ continue; }} }}
function walkBreak2(): void {{
    foreach ([1] as $outer) {{
        foreach (new FreshAgg() as $v) {{ break 2; }}
    }}
}}
function walkReturn(): int {{
    foreach (new FreshAgg() as $v) {{ return $v; }}
    return 0;
}}
"
    );
    for target in TARGETS {
        for name in ["walkEnd", "walkBreak", "walkContinue", "walkBreak2", "walkReturn"] {
            assert_single_owner_lifetime(&lower_function(target, &source, name), target);
        }
        let r#break = lower_function(target, &source, "walkBreak");
        let continue_fn = lower_function(target, &source, "walkContinue");
        let (_, owner) = owned_iter_start(&r#break);
        // Innermost break branches to the exit that already retires; the break
        // block itself must not emit a second pop/release.
        let break_block = r#break
            .blocks
            .iter()
            .find(|block| {
                block.instructions.iter().any(|id| {
                    let inst = &r#break.instructions[id.as_raw() as usize];
                    inst.op == Op::IterCurrentValue
                })
            })
            .expect("foreach body exists");
        assert!(
            !break_block.instructions.iter().any(|id| {
                let inst = &r#break.instructions[id.as_raw() as usize];
                matches!(inst.op, Op::PopCallOperandOwner | Op::ReleaseLocalSlot)
                    && inst.immediate == Some(Immediate::LocalSlot(owner))
            }),
            "{target}: innermost break must not retire the owner in the body"
        );
        let (_, continue_owner) = owned_iter_start(&continue_fn);
        let continue_header = continue_fn
            .blocks
            .iter()
            .find(|block| block.name == "foreach.next")
            .expect("foreach header exists");
        assert!(
            !continue_header.instructions.iter().any(|id| {
                let inst = &continue_fn.instructions[id.as_raw() as usize];
                matches!(inst.op, Op::PopCallOperandOwner | Op::ReleaseLocalSlot)
                    && inst.immediate == Some(Immediate::LocalSlot(continue_owner))
            }),
            "{target}: continue must not retire the owner in the header"
        );
    }
}

/// Same-frame throws from iterator methods still name a live owner at IterStart.
#[test]
fn throwing_iterator_methods_keep_a_published_owner_on_every_target() {
    let source = format!(
        "<?php {PRELUDE}
class BoomRewind extends RangeIter {{
    public function rewind(): void {{ throw new RuntimeException(\"rewind\"); }}
}}
class BoomCurrent extends RangeIter {{
    public function current(): mixed {{ throw new RuntimeException(\"current\"); }}
}}
class BoomKey extends RangeIter {{
    public function key(): mixed {{ throw new RuntimeException(\"key\"); }}
}}
class BoomNext extends RangeIter {{
    public function next(): void {{ throw new RuntimeException(\"next\"); }}
}}
class AggRewind implements IteratorAggregate {{
    public function getIterator(): Iterator {{ return new BoomRewind(1); }}
}}
class AggCurrent implements IteratorAggregate {{
    public function getIterator(): Iterator {{ return new BoomCurrent(1); }}
}}
class AggKey implements IteratorAggregate {{
    public function getIterator(): Iterator {{ return new BoomKey(1); }}
}}
class AggNext implements IteratorAggregate {{
    public function getIterator(): Iterator {{ return new BoomNext(2); }}
}}
function walkRewind(): void {{ try {{ foreach (new AggRewind() as $v) {{ echo $v; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }} }}
function walkCurrent(): void {{ try {{ foreach (new AggCurrent() as $k => $v) {{ echo $v; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }} }}
function walkKey(): void {{ try {{ foreach (new AggKey() as $k => $v) {{ echo $v; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }} }}
function walkNext(): void {{ try {{ foreach (new AggNext() as $v) {{ echo $v; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }} }}
"
    );
    for target in TARGETS {
        for name in ["walkRewind", "walkCurrent", "walkKey", "walkNext"] {
            assert_single_owner_lifetime(&lower_function(target, &source, name), target);
        }
    }
}

/// Descriptor unpack of an aggregate publishes the owner inside the existing LIFO stack.
#[test]
fn descriptor_unpack_aggregate_publishes_owner_on_every_target() {
    let source = format!(
        "<?php {PRELUDE}
class UnpackTarget {{
    public function add(int $first, int $second): int {{ return $first + $second; }}
}}
function unpackAggregate(): IteratorAggregate {{ return new FreshAgg(); }}
function callUnpacked(callable $callback): mixed {{
    return $callback(...unpackAggregate());
}}
"
    );
    for target in TARGETS {
        assert_single_owner_lifetime(&lower_function(target, &source, "callUnpacked"), target);
    }
}

/// Arrays, direct Iterator, and generators must keep IterStart owner-free.
#[test]
fn arrays_iterators_and_generators_do_not_allocate_an_owner_on_every_target() {
    let source = format!(
        "<?php {PRELUDE}
function walkArray(): void {{ foreach ([1, 2] as $v) {{ echo $v; }} }}
function walkIter(): void {{ foreach (new RangeIter(2) as $v) {{ echo $v; }} }}
function gen() {{ yield 1; }}
function walkGen(): void {{ foreach (gen() as $v) {{ echo $v; }} }}
"
    );
    for target in TARGETS {
        for name in ["walkArray", "walkIter", "walkGen"] {
            let function = lower_function(target, &source, name);
            assert!(
                iter_start_owners(&function).iter().all(|(_, owner)| owner.is_none()),
                "{target}/{name}: must not allocate a getIterator owner"
            );
        }
    }
}

/// Iterator protocol opcodes retain arbitrary callback effects until calls are explicit in EIR.
#[test]
fn iterator_callback_opcodes_are_conservatively_effectful() {
    for op in [Op::IterStart, Op::IterNext, Op::IterCurrentKey, Op::IterCurrentValue] {
        assert_eq!(op.default_effects(), Effects::all(), "{op:?}");
    }
    assert_ne!(Op::IterCurrentValueRef.default_effects(), Effects::all());
}

/// Builds one otherwise valid function whose IterStart names the supplied local.
fn function_with_iter_owner_local(
    kind: LocalKind,
    ir_type: IrType,
    php_type: PhpType,
) -> Function {
    let mut function = Function::new("bad_owner_shape".to_string(), IrType::Void, PhpType::Void);
    let owner = function.add_local(Some("owner".to_string()), ir_type, php_type, kind);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let source = builder
            .emit(
                Op::ArrayNew,
                Vec::new(),
                None,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        builder
            .emit(
                Op::IterStart,
                vec![source],
                Some(Immediate::IterStart {
                    by_ref: false,
                    owner: Some(owner),
                }),
                IrType::Heap(IrHeapKind::Iterable),
                PhpType::Iterable,
                Ownership::MaybeOwned,
            )
            .expect("iter_start produces a value");
        builder.terminate(Terminator::Return { value: None });
    }
    function
}

/// Existing locals are not valid owners unless both role and storage are the Mixed owner shape.
#[test]
fn validator_rejects_wrong_kind_and_wrong_type_iter_start_owners() {
    let wrong_kind = function_with_iter_owner_local(
        LocalKind::PhpLocal,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    assert!(validate_function(&wrong_kind).is_err());

    let wrong_type =
        function_with_iter_owner_local(LocalKind::OwnedTemp, IrType::I64, PhpType::Int);
    assert!(validate_function(&wrong_type).is_err());
}

/// IterStart no longer accepts the old absent or boolean immediate forms.
#[test]
fn validator_rejects_legacy_iter_start_immediates() {
    for immediate in [None, Some(Immediate::Bool(false)), Some(Immediate::Bool(true))] {
        let mut function = Function::new("legacy_iter_start".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let source = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    None,
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("array_new produces a value");
            builder
                .emit(
                    Op::IterStart,
                    vec![source],
                    immediate,
                    IrType::Heap(IrHeapKind::Iterable),
                    PhpType::Iterable,
                    Ownership::MaybeOwned,
                )
                .expect("iter_start produces a value");
            builder.terminate(Terminator::Return { value: None });
        }
        assert!(validate_function(&function).is_err());
    }
}

/// An out-of-range IterStart owner is rejected by the validator on a hand-built function.
#[test]
fn validator_rejects_an_unknown_iter_start_owner_slot() {
    let mut function = Function::new("bad_owner".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let source = builder
            .emit(
                Op::ArrayNew,
                Vec::new(),
                None,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        builder
            .emit(
                Op::IterStart,
                vec![source],
                Some(Immediate::IterStart {
                    by_ref: false,
                    owner: Some(LocalSlotId::from_raw(99)),
                }),
                IrType::Heap(IrHeapKind::Iterable),
                PhpType::Iterable,
                Ownership::MaybeOwned,
            )
            .expect("iter_start produces a value");
        builder.terminate(Terminator::Return { value: None });
    }
    let error = validate_function(&function).expect_err("unknown owner slot is invalid");
    assert!(
        matches!(
            error,
            crate::ir::ValidationError::MissingImmediate {
                expected: "valid iter_start owner local slot",
                ..
            }
        ),
        "got {error:?}"
    );
}
