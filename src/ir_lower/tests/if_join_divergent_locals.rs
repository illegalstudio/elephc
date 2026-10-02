//! Purpose:
//! Pins how EIR lowering joins a local whose `if` arms leave different representations
//! (issue #771), and the deferred slot release that keeps the join's edge boxing leak-free.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - The fixtures put the `if` in a closure body, which DCE's tail-sinking leaves alone; in a named
//!   function it would copy the trailing read into both arms and hide the join.
//! - Slots are found by their PHP name, so the assertions do not depend on slot numbering.

use crate::ir::{Function, Immediate, LocalSlotId, Module, Op};
use crate::types::PhpType;

/// Returns the only lowered closure body in `module`.
fn only_closure(module: &Module) -> &Function {
    let [closure] = module.closures.as_slice() else {
        panic!(
            "expected exactly one closure, got {:?}",
            module.closures.iter().map(|function| &function.name).collect::<Vec<_>>()
        );
    };
    closure
}

/// Returns the frame slot `function` allocated for the PHP local `name`.
fn named_slot(function: &Function, name: &str) -> LocalSlotId {
    function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("{}: missing local ${name}", function.name))
        .id
}

/// Returns the PHP result types of every `load_local` of `slot`, in instruction order.
fn load_types(function: &Function, slot: LocalSlotId) -> Vec<PhpType> {
    function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::LoadLocal && inst.immediate == Some(Immediate::LocalSlot(slot)))
        .map(|inst| inst.result_php_type.clone())
        .collect()
}

/// Counts the `release_local_slot` ops on `slot` that survived the untracked-slot prune.
fn surviving_slot_releases(function: &Function, slot: LocalSlotId) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .count()
}

/// Counts the `store_local`s of an int into `slot` with no surviving `release_local_slot` before
/// them in the same block.
///
/// In the fixture every int store overwrites an initialized slot (the local is `null` first),
/// and the slot ends as boxed storage, so the backend boxes these stores and each one replaces a
/// cell that only the deferred release frees.
fn unreleased_int_overwrites(function: &Function, slot: LocalSlotId) -> usize {
    let targets_slot = |immediate: &Option<Immediate>| *immediate == Some(Immediate::LocalSlot(slot));
    let mut unreleased = 0;
    for block in &function.blocks {
        let mut released = false;
        for inst_id in &block.instructions {
            let inst = function.instruction(*inst_id).expect("block references a valid instruction");
            if inst.op == Op::ReleaseLocalSlot && targets_slot(&inst.immediate) {
                released = true;
            } else if inst.op == Op::StoreLocal && targets_slot(&inst.immediate) {
                let stores_int = inst
                    .operands
                    .first()
                    .and_then(|operand| function.value(*operand))
                    .is_some_and(|value| value.php_type == PhpType::Int);
                if stores_int && !released {
                    unreleased += 1;
                }
                released = false;
            }
        }
    }
    unreleased
}

/// The #771 repro: the read after the merge is typed `mixed`, never the fall-through arm's `null`.
#[test]
fn closure_null_then_string_join_reads_the_local_as_mixed() {
    let module = super::lower_source(
        r#"<?php
$m = "k" . $argc;
$f = function (int $n) use ($m) { $m = null; if ($n > 1) { $m = "s" . $n; } return $m; };
var_dump($f(2));
"#,
    );
    let closure = only_closure(&module);
    let slot = named_slot(closure, "m");
    let loads = load_types(closure, slot);
    assert!(
        loads.contains(&PhpType::Mixed),
        "the post-merge read must be boxed, got {loads:?}"
    );
    assert!(
        !loads.contains(&PhpType::Void),
        "no read may see the slot as `null` after the merge, got {loads:?}"
    );
}

/// A `null`-or-int local shares one scalar word, so each arm boxes it on its merge edge, and the
/// arm's own scalar overwrite keeps the deferred release that frees the box it replaces.
#[test]
fn null_or_int_join_boxes_on_edges_and_keeps_the_arm_release() {
    let module = super::lower_source(
        r#"<?php
$pick = function (int $n) { $v = null; if ($n > 0) { $v = $n; $v = $n & 3; } return $v; };
var_dump($pick(1));
"#,
    );
    let pick = only_closure(&module);
    let slot = named_slot(pick, "v");
    assert_eq!(
        pick.locals[slot.as_raw() as usize].php_type,
        PhpType::Mixed,
        "edge boxing widens the slot to boxed storage"
    );
    assert!(
        pick.instructions.iter().any(|inst| inst.op == Op::MixedBox),
        "each arm must box its value on the merge edge"
    );
    assert_eq!(
        unreleased_int_overwrites(pick, slot),
        0,
        "every int overwrite of the slot must retire the box it replaces"
    );
    assert!(
        load_types(pick, slot).contains(&PhpType::Mixed),
        "the read after the merge is boxed"
    );
}

/// Arms that agree on a scalar representation keep the scalar slot, and the deferred releases
/// emitted for its overwrites are all pruned away.
#[test]
fn agreeing_scalar_arms_keep_a_scalar_slot_without_releases() {
    let module = super::lower_source(
        r#"<?php
$keep = function (int $n) { $v = 0; if ($n > 0) { $v = $n; $v = $n & 3; } return $v; };
var_dump($keep(1));
"#,
    );
    let keep = only_closure(&module);
    let slot = named_slot(keep, "v");
    assert_eq!(keep.locals[slot.as_raw() as usize].php_type, PhpType::Int);
    assert_eq!(
        surviving_slot_releases(keep, slot),
        0,
        "a slot that stays scalar must not keep any deferred release"
    );
}
