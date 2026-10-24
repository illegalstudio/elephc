//! Purpose:
//! Guards per-binding frame storage for a loop counter whose name a later loop reuses.
//!
//! Called from:
//! - The AST-to-EIR library regression suite.
//!
//! Key details:
//! - Counter storage is checked before optional optimization on every supported target.
//! - The provably bounded binding narrows to `I64` loads; the binding seeded by a checked
//!   `int - int` stays boxed, and ending the first binding emits no slot-zeroing opcode that
//!   would hide its slot from the numeric proofs.

use crate::ir::{Function, Immediate, IrHeapKind, IrType, LocalSlotId, Op};

/// An ascending loop followed by a descending loop seeded with a checked subtraction.
const ASCENDING_THEN_DESCENDING: &str = r#"<?php
function reusedCounter(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += 1; }
    for ($i = $n - 1; $i >= 0; $i--) { $sum += 1; }
    return $sum;
}
echo reusedCounter($argc + 9);
"#;

/// The same two loops in the opposite order: the integer binding is the second one.
const DESCENDING_THEN_ASCENDING: &str = r#"<?php
function reusedCounter(int $n): int {
    $sum = 0;
    for ($i = $n - 1; $i >= 0; $i--) { $sum += 1; }
    for ($i = 0; $i < $n; $i++) { $sum += 1; }
    return $sum;
}
echo reusedCounter($argc + 9);
"#;

/// Returns every frame slot bound to one PHP name, in binding order.
fn bindings(function: &Function, name: &str) -> Vec<LocalSlotId> {
    function
        .locals
        .iter()
        .filter(|local| local.name.as_deref() == Some(name))
        .map(|local| local.id)
        .collect()
}

/// Collects the physical load representations of one slot.
fn loads(function: &Function, slot: LocalSlotId) -> Vec<IrType> {
    function
        .instructions
        .iter()
        .filter(|inst| {
            inst.op == Op::LoadLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
        })
        .map(|inst| inst.result_type)
        .collect()
}

/// Lowers `source` for `target` and checks which binding of `$i` stays scalar.
fn assert_scalar_binding(source: &str, target: &str, scalar: usize) {
    let module = super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "reusedCounter")
        .unwrap();
    let printed = crate::ir::print_function(function);
    let slots = bindings(function, "i");
    assert_eq!(slots.len(), 2, "{target}: each loop owns its binding of $i:\n{printed}");
    let integer = loads(function, slots[scalar]);
    assert!(
        !integer.is_empty() && integer.iter().all(|ty| *ty == IrType::I64),
        "{target}: the bounded binding loads raw integers:\n{printed}"
    );
    assert!(
        loads(function, slots[1 - scalar]).contains(&IrType::Heap(IrHeapKind::Mixed)),
        "{target}: the binding seeded by `$n - 1` keeps its boxed storage:\n{printed}"
    );
    assert!(
        !function.instructions.iter().any(|inst| {
            inst.op == Op::ZeroLocalSlot && inst.immediate == Some(Immediate::LocalSlot(slots[0]))
        }),
        "{target}: ending the first binding needs no zero_local_slot:\n{printed}"
    );
}

/// The ascending loop keeps an integer counter although a later loop reuses `$i`.
#[test]
fn reused_loop_counter_keeps_the_earlier_bounded_binding_scalar_on_all_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        assert_scalar_binding(ASCENDING_THEN_DESCENDING, target, 0);
    }
}

/// The ascending loop keeps an integer counter although an earlier loop seeded `$i` boxed.
#[test]
fn reused_loop_counter_keeps_the_later_bounded_binding_scalar_on_all_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        assert_scalar_binding(DESCENDING_THEN_ASCENDING, target, 1);
    }
}
