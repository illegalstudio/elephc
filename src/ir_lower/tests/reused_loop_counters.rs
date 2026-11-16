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

/// Nested loops with disjoint uses of a name get separate storage on every target.
#[test]
fn numeric_region_nested_counter_has_independent_storage() {
    let source = r#"<?php
function nestedCounter(int $n): int {
    $sum = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($row = $n - 1; $row >= 0; $row--) { $sum += 1; }
        for ($row = 0; $row < $n; $row++) { $sum += 1; }
    }
    return $sum;
}
echo nestedCounter($argc + 2);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|f| f.name == "nestedCounter").unwrap();
        let slots = bindings(function, "row");
        let printed = crate::ir::print_function(function);
        assert_eq!(slots.len(), 2, "{target}: {printed}");
        assert!(loads(function, slots[1]).iter().all(|ty| *ty == IrType::I64), "{target}: {printed}");
    }
}

/// A use at loop exit or before the initializer prevents conditional name rebinding.
#[test]
fn numeric_region_live_counter_keeps_shared_storage() {
    for extra in ["echo $row;", "if ($outer > 0) { echo $row; }"] {
        let source = format!(r#"<?php
function liveCounter(int $n): int {{
    for ($outer = 0; $outer < $n; $outer++) {{
        for ($row = $n - 1; $row >= 0; $row--) {{}}
        {extra}
        for ($row = 0; $row < $n; $row++) {{}}
    }}
    return 0;
}}
echo liveCounter($argc);
"#);
        let module = super::lower_source(&source);
        let function = module.functions.iter().find(|f| f.name == "liveCounter").unwrap();
        assert_eq!(bindings(function, "row").len(), 1, "{}", crate::ir::print_function(function));
    }
}

/// Unknown Mixed seeds and captured names cannot acquire region-scoped numeric storage.
#[test]
fn numeric_region_unknown_seed_and_capture_keep_shared_storage() {
    for (seed, capture) in [("$seed", ""), ("$n - 1", "$copy = static fn() => $row;")] {
        let source = format!(r#"<?php
function guardedCounter(int $n, mixed $seed): int {{
    for ($outer = 0; $outer < $n; $outer++) {{
        for ($row = {seed}; $row >= 0; $row--) {{}}
        {capture}
        for ($row = 0; $row < $n; $row++) {{}}
    }}
    return 0;
}}
echo guardedCounter($argc, $argc);
"#);
        let module = super::lower_source(&source);
        let function = module.functions.iter().find(|f| f.name == "guardedCounter").unwrap();
        assert_eq!(bindings(function, "row").len(), 1, "{}", crate::ir::print_function(function));
    }
}

/// A boxed numeric counter can seed a separate arithmetic index binding between counter regions.
#[test]
fn numeric_region_index_binding_accepts_closed_numeric_dependencies() {
    let module = super::lower_source(r#"<?php
function indexRegions(int $n): int {
    for ($outer = 0; $outer < $n; $outer++) {
        for ($cell = 0; $cell < $n; $cell++) {}
        for ($row = 0; $row <= $n; $row++) {
            $cell = $row * 3 + 1;
            echo (int)$cell;
        }
        for ($cell = 0; $cell < $n; $cell++) {}
    }
    return 0;
}
echo indexRegions($argc);
"#);
    let function = module.functions.iter().find(|f| f.name == "indexRegions").unwrap();
    assert_eq!(bindings(function, "cell").len(), 3, "{}", crate::ir::print_function(function));
}

/// A later unknown writer retracts earlier decisions for every dependent arithmetic binding.
#[test]
fn numeric_region_late_unknown_writer_retracts_dependent_splits() {
    let source = r#"<?php
function dependentRegions(int $n, mixed $seed): int {
    for ($outer = 0; $outer < $n; $outer++) {
        for ($row = 0; $row < $n; $row++) {
            $cell = $row + 1;
            echo (int)$cell;
            $row = $seed;
        }
        for ($cell = 0; $cell < $n; $cell++) {}
    }
    return 0;
}
echo dependentRegions($argc, $argc);
"#;
    for mutation in ["$row = $seed;", "$callback = function() use (&$row) {}; "] {
        let module = super::lower_source(&source.replace("$row = $seed;", mutation));
        let function = module.functions.iter().find(|f| f.name == "dependentRegions").unwrap();
        assert_eq!(bindings(function, "cell").len(), 1, "{}", crate::ir::print_function(function));
    }
}

/// Numeric property leaves require a declared type before they authorize region splitting.
#[test]
fn numeric_region_index_property_requires_declared_numeric_type() {
    for (declaration, expected) in [("int", 3), ("", 1)] {
        let source = format!(r#"<?php
final class RegionProperties {{
    private {declaration} $columns = 3;
    public function indexRegions(int $n): int {{
        for ($outer = 0; $outer < $n; $outer++) {{
            for ($cell = 0; $cell < $n; $cell++) {{}}
            for ($row = 0; $row <= $n; $row++) {{
                $cell = $row * $this->columns + 1;
                echo (int)$cell;
            }}
            for ($cell = 0; $cell < $n; $cell++) {{}}
        }}
        return 0;
    }}
}}
echo (new RegionProperties())->indexRegions($argc);
"#);
        let module = super::lower_source(&source);
        let function = module.class_methods.iter().find(|f| f.name.contains("indexRegions")).unwrap();
        assert_eq!(bindings(function, "cell").len(), expected, "{}", crate::ir::print_function(function));
    }
}

/// Retracting region-only decisions must preserve the independent straight-line numeric split.
#[test]
fn numeric_region_nonnumeric_tail_preserves_existing_counter_split() {
    let source = ASCENDING_THEN_DESCENDING.replace("return $sum;", "$i = \"done\"; return $sum;");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        assert_scalar_binding(&source, target, 0);
    }
}
