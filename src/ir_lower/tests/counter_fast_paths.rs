//! Purpose:
//! Verifies guarded counter copies, dead seed overflow and arithmetic through index locals.
//!
//! Called from:
//! - The AST-to-EIR library regression suite.
//!
//! Key details:
//! - Every assertion inspects the named user function, avoiding injected prelude counts.

use crate::ir::{Function, Op};

/// Lowers and optimizes one fixture for a supported target, returning its user function.
fn optimized(source: &str, name: &str, target: &str) -> Function {
    let mut module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    crate::ir_passes::optimize_module(&mut module);
    module.functions.into_iter().find(|f| f.name == name).unwrap()
}

/// Inclusive integer bounds get both a scalar loop and the original overflow-capable fallback.
#[test]
fn counter_fast_path_inclusive_loop_has_integer_version_on_all_targets() {
    let source = r#"<?php
function inclusiveCounter(int $start, int $end): int {
    $count = 0;
    for ($i = $start; $i <= $end; $i++) { $count += 1; }
    return $count;
}
echo inclusiveCounter($argc, $argc + 4);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let function = optimized(source, "inclusiveCounter", target);
        let text = crate::ir::print_function(&function);
        assert!(text.contains(".counter.fast"), "{target}: {text}");
        assert!(function.instructions.iter().any(|i| i.op == Op::StrIncDec), "{target}: {text}");
        assert!(function.instructions.iter().any(|i| i.op == Op::IAdd), "{target}: {text}");
    }
}

/// A checked product used as a stable loop bound is guarded by its concrete runtime tag.
#[test]
fn counter_fast_path_checked_bound_has_integer_version() {
    let source = r#"<?php
function productCounter(int $a, int $b): int {
    $cells = $a * $b;
    $count = 0;
    for ($cell = 0; $cell < $cells; $cell++) { $count += 1; }
    return $count;
}
echo productCounter($argc, $argc + 4);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let function = optimized(source, "productCounter", target);
        let text = crate::ir::print_function(&function);
        assert!(text.contains(".counter.fast"), "{target}: {text}");
        let entry = function.blocks.iter().find(|b| b.name == "counter.integer.entry").unwrap();
        assert!(entry.instructions.iter().any(|id| function.instruction(*id).unwrap().op == Op::MixedUnbox),
            "{target}: guarded payloads use unboxing:\n{text}");
        assert!(!entry.instructions.iter().any(|id| function.instruction(*id).unwrap().op == Op::Cast),
            "{target}: guards must not add PHP cast effects:\n{text}");
    }
}

/// A seed below PHP_INT_MIN is unobservable when a dead-at-exit descending loop never enters.
#[test]
fn counter_fast_path_descending_seed_skips_overflow_without_loop_copy() {
    let source = r#"<?php
function descendingCounter(int $n): int {
    $count = 0;
    for ($box = $n - 1; $box >= 0; $box--) { $count += 1; }
    return $count;
}
echo descendingCounter($argc + 4);
"#;
    let function = optimized(source, "descendingCounter", "linux-x86_64");
    let text = crate::ir::print_function(&function);
    assert!(text.contains("counter.seed.inrange"), "{text}");
    assert!(!function.instructions.iter().any(|i| i.op == Op::StrIncDec), "{text}");
}

/// An index's suffix remains inside the original float-promoting arithmetic chain.
#[test]
fn counter_fast_path_index_chain_crosses_private_local() {
    let source = r#"<?php
function cellIndex(int $row, int $columns, int $column): int {
    $cell = $row * $columns + $column;
    return (int)($cell + 1);
}
echo cellIndex($argc, $argc + 4, $argc);
"#;
    let function = optimized(source, "cellIndex", "linux-x86_64");
    let text = crate::ir::print_function(&function);
    assert!(text.contains("ichecked_numeric_chain_to_int"), "{text}");
    assert!(!function.instructions.iter().any(|i| i.op == Op::MixedNumericBinop), "{text}");
}

/// A changing bound cannot be frozen at loop entry by integer loop versioning.
#[test]
fn counter_fast_path_mutable_bound_keeps_original_loop() {
    let source = r#"<?php
function changingCounter(int $end): int {
    $count = 0;
    for ($i = 0; $i <= $end; $i++) { $end -= 1; $count += 1; }
    return $count;
}
echo changingCounter($argc + 4);
"#;
    let function = optimized(source, "changingCounter", "linux-x86_64");
    assert!(!crate::ir::print_function(&function).contains(".counter.fast"));
}

/// A data slot shared by both loop copies can fuse its integer writer while retaining the fallback.
#[test]
fn counter_fast_path_index_inside_versioned_loop_fuses_integer_copy() {
    let source = r#"<?php
function guardedIndex(int $end, int $columns, int $column): int {
    for ($row = 0; $row <= $end; $row++) {
        $cell = $row * $columns + $column;
        echo (int)($cell + 1);
    }
    return 0;
}
echo guardedIndex($argc, 31, 7);
"#;
    let function = optimized(source, "guardedIndex", "linux-x86_64");
    let text = crate::ir::print_function(&function);
    assert!(text.contains(".counter.fast"), "{text}");
    assert!(function.instructions.iter().any(|i| i.op == Op::ICheckedNumericChainToInt), "{text}");
    assert!(function.instructions.iter().any(|i| i.op == Op::MixedNumericBinop), "{text}");
}

/// A post-test loop cannot use its later comparison to justify an earlier unchecked update.
#[test]
fn counter_fast_path_post_test_keeps_overflow_capable_update() {
    let function = optimized(r#"<?php
function postTest(int $start, int $end): mixed {
    $i = $start;
    do { $i++; } while ($i < $end);
    return $i;
}
echo postTest($argc, 0);
"#, "postTest", "linux-x86_64");
    let text = crate::ir::print_function(&function);
    assert!(!text.contains(".counter.fast"), "{text}");
    assert!(function.instructions.iter().any(|i| i.op == Op::StrIncDec), "{text}");
}

/// A comparison evaluated before the loop does not constrain later counter updates.
#[test]
fn counter_fast_path_invariant_condition_keeps_generic_loop() {
    let function = optimized(r#"<?php
function invariantCondition(int $n, int $start): void {
    $i = $start + 0 * $n;
    $c = $i < $n;
    $k = 0;
    while ($c) { $i++; $k++; if ($k >= 3) { break; } }
    echo $i;
}
invariantCondition(PHP_INT_MAX, $argc);
"#, "invariantCondition", "linux-x86_64");
    let text = crate::ir::print_function(&function);
    assert!(!text.contains(".counter.fast"), "{text}");
}

/// Repeated inner updates cannot be justified by one outer-loop comparison.
#[test]
fn counter_fast_path_nested_update_keeps_checked_seed() {
    let function = optimized(r#"<?php
function nestedUpdates(int $n, int $lo): int {
    $count = 0;
    for ($i = $n - 1; $i > $lo; $count++) {
        for ($k = 0; $k < 2; $k++) { $i--; }
        if ($count > 2) { break; }
    }
    return $count;
}
echo nestedUpdates($argc, PHP_INT_MIN);
"#, "nestedUpdates", "linux-x86_64");
    let text = crate::ir::print_function(&function);
    assert!(!text.contains("counter.seed.inrange"), "{text}");
    assert!(function.instructions.iter().any(|i| i.op == Op::StrIncDec), "{text}");
}

/// A binding written only in an arm is retired safely at its later region on every target.
#[test]
fn counter_fast_path_conditional_region_retires_maybe_written_owner() {
    let source = r#"<?php
function conditionalRegion(int $n, bool $c): int {
    $count = 0;
    for ($r = 0; $r < $n; $r++) { $count += 1; }
    if ($c) { for ($r = $n - 1; $r >= 0; $r--) { $count += 1; } }
    for ($r = 0; $r < $n; $r++) { $count += 1; }
    return $count;
}
echo conditionalRegion($argc + 2, true);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|f| f.name == "conditionalRegion").unwrap();
        let merge = function.blocks.iter().find(|b| b.name == "if.merge").unwrap();
        assert!(merge.instructions.iter().any(|id| function.instructions[id.as_raw() as usize].op == Op::ReleaseLocalSlot),
            "{target}: {}", crate::ir::print_function(function));
        assert!(!merge.instructions.iter().any(|id| function.instructions[id.as_raw() as usize].op == Op::ZeroLocalSlot),
            "{target}: {}", crate::ir::print_function(function));
    }
}

/// Coercing property stores retain their boxed counter inputs in both candidate loop shapes.
#[test]
fn counter_fast_path_coercing_stores_preserve_boxed_inputs_on_all_targets() {
    let source = r#"<?php
class CounterDestination { public static float $last = 0.0; }
function coercingBound(mixed $end): int {
    $count = 0;
    for ($i = 0; $i < $end; $i++) { CounterDestination::$last = $i; $count++; }
    return $count;
}
function coercingSeed(int $n): int {
    $count = 0;
    for ($box = $n - 1; $box >= 0; $box--) { CounterDestination::$last = $box; $count++; }
    return $count;
}
echo coercingBound($argc + 2), coercingSeed($argc + 3);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for name in ["coercingBound", "coercingSeed"] {
            let function = optimized(source, name, target);
            let text = crate::ir::print_function(&function);
            let stores: Vec<_> = function.instructions.iter().filter(|inst| inst.op == Op::StoreStaticProperty).collect();
            assert!(!stores.is_empty(), "{target}: {text}");
            for store in stores {
                assert!(store.operands.iter().all(|value| function.values[value.as_raw() as usize].ir_type
                    == crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed)), "{target}: {text}");
            }
        }
    }
}

/// Boxed array casts and PHP boolean/null ordering retain compatible counter operands on every target.
#[test]
fn counter_fast_path_boxed_cast_and_ordering_keep_original_inputs() {
    use crate::ir::{Immediate, IrHeapKind, IrType};
    let source = r#"<?php
function arrayCounterBound(int $start, mixed $end): int {
    $hits = 0;
    for ($i = $start; $i <= $end; $i++) { $a = (array)$i; $hits += $a[0]; }
    return $hits;
}
function arrayCounterSeed(int $n): int {
    $hits = 0;
    for ($box = $n - 1; $box >= 0; $box--) { $a = (array)$box; $hits += $a[0]; }
    return $hits;
}
function orderingCounterBound(int $start, mixed $end): int {
    $hits = 0;
    for ($i = $start; $i <= $end; $i++) { $hits += ($i <=> true); $hits += ($i <=> null); }
    return $hits;
}
function orderingCounterSeed(int $n, int $lo): int {
    $hits = 0;
    for ($box = $n - 1; $box > $lo; $box--) { $hits += ($box <=> true); $hits += ($box <=> null); }
    return $hits;
}
echo arrayCounterBound($argc, $argc + 3), arrayCounterSeed($argc + 3);
echo orderingCounterBound(-2 + $argc, $argc + 3), orderingCounterSeed($argc + 3, -2);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for (name, arrays) in [("arrayCounterBound", true), ("arrayCounterSeed", true),
            ("orderingCounterBound", false), ("orderingCounterSeed", false)] {
            let function = optimized(source, name, target);
            let text = crate::ir::print_function(&function);
            let consumers: Vec<_> = function.instructions.iter().filter(|inst| {
                if arrays { inst.op == Op::Cast && inst.immediate == Some(Immediate::CastTarget(IrType::Heap(IrHeapKind::Mixed))) }
                else { inst.op == Op::Spaceship }
            }).collect();
            assert!(!consumers.is_empty(), "{target}: {text}");
            for consumer in consumers {
                assert_eq!(function.values[consumer.operands[0].as_raw() as usize].ir_type,
                    IrType::Heap(IrHeapKind::Mixed), "{target}: {text}");
            }
        }
    }
}

/// Strict identity with a raw pointer retains the counter's boxed representation on every target.
#[test]
fn counter_fast_path_pointer_identity_keeps_boxed_inputs() {
    let source = r#"<?php
function pointerIdentityBound(int $start, mixed $end): int {
    $p = ptr_null(); $hits = 0;
    for ($i = $start; $i <= $end; $i++) {
        if ($i === $p) { $hits++; }
        if ($i !== $p) { $hits += 10; }
    }
    return $hits;
}
function pointerIdentitySeed(int $n): int {
    $p = ptr_null(); $hits = 0;
    for ($box = $n - 1; $box >= 0; $box--) {
        if ($box === $p) { $hits++; }
        if ($box !== $p) { $hits += 10; }
    }
    return $hits;
}
echo pointerIdentityBound($argc - 1, $argc + 1), pointerIdentitySeed($argc + 2);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for name in ["pointerIdentityBound", "pointerIdentitySeed"] {
            let function = optimized(source, name, target);
            let text = crate::ir::print_function(&function);
            let comparisons: Vec<_> = function.instructions.iter()
                .filter(|inst| matches!(inst.op, Op::StrictEq | Op::StrictNotEq)).collect();
            assert!(comparisons.len() >= 2, "{target}: {text}");
            for comparison in comparisons {
                assert_eq!(function.values[comparison.operands[0].as_raw() as usize].ir_type,
                    crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed), "{target}: {text}");
            }
        }
    }
}

/// Every eligible seed is guarded before full loop copies consume their separate growth budget.
#[test]
fn counter_fast_path_multiple_seeds_use_guards_without_copies() {
    for count in [2, 10] {
        let loops: String = (0..count).map(|index| format!(
            "for ($b{index} = $n - 1; $b{index} >= 0; $b{index}--) {{ $count += 1; }}\n"
        )).collect();
        let source = format!("<?php function multipleSeeds(int $n): int {{ $count = 0; {loops} return $count; }} echo multipleSeeds($argc + 2);");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let function = optimized(&source, "multipleSeeds", target);
            let guards = function.blocks.iter().filter(|block| block.name == "counter.seed.inrange").count();
            assert_eq!(guards, count, "{target}: each descending loop gets its own seed guard");
            assert!(!function.blocks.iter().any(|block| block.name.contains(".counter.fast")),
                "{target}: seed-only loops do not need full copies");
            assert!(!function.instructions.iter().any(|inst| inst.op == Op::StrIncDec),
                "{target}: no eligible seed counter remains boxed");
        }
    }
}

/// Loose comparisons with potentially boxed objects keep the full PHP comparison on every target.
#[test]
fn counter_fast_path_loose_object_comparisons_keep_boxed_inputs() {
    let source = r#"<?php
function looseObjectBound(mixed $end, mixed $other): int {
    $hits = 0;
    for ($i = 0; $i < $end; $i++) {
        if ($i == $other) { $hits++; }
        if ($i != $other) { $hits += 10; }
    }
    return $hits;
}


function looseObjectSeed(int $n, ?stdClass $other): int {
    $hits = 0;
    for ($box = $n - 1; $box >= 0; $box--) {
        if ($box == $other) { $hits++; }
        if ($box != $other) { $hits += 10; }
    }
    return $hits;
}
echo looseObjectBound($argc + 2, new stdClass), looseObjectSeed($argc + 2, new stdClass);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for name in ["looseObjectBound", "looseObjectSeed"] {
            let function = optimized(source, name, target);
            let text = crate::ir::print_function(&function);
            let comparisons: Vec<_> = function.instructions.iter()
                .filter(|inst| matches!(inst.op, Op::LooseEq | Op::LooseNotEq)).collect();
            assert!(comparisons.len() >= 2, "{target}: {text}");
            for comparison in comparisons {
                assert_eq!(function.values[comparison.operands[0].as_raw() as usize].ir_type,
                    crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed), "{target}: {text}");
            }
        }
    }
}

/// Numeric-region replay retains boxed inputs for every representation-sensitive comparison family.
#[test]
fn counter_fast_path_region_comparison_partners_keep_boxed_inputs() {
    let source = r#"<?php
function regionObject(int $n, mixed $other): int {
    $hits = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($i = $n - 1; $i >= 0; $i--) {}
        for ($i = 0; $i < $n; $i++) { if ($i == $other) { $hits++; } if ($i != $other) { $hits++; } }
    }
    return $hits;
}
function regionOrdering(int $n): int {
    $hits = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($i = $n - 1; $i >= 0; $i--) {}
        for ($i = -2; $i < $n; $i++) { $hits += ($i <=> true); $hits += ($i <=> null); }
    }
    return $hits;
}
function regionPointer(int $n): int {
    $p = ptr_null(); $hits = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($i = $n - 1; $i >= 0; $i--) {}
        for ($i = 0; $i < $n; $i++) { if ($i === $p) { $hits++; } if ($i !== $p) { $hits++; } }
    }
    return $hits;
}
echo regionObject($argc + 2, new stdClass), regionOrdering($argc + 2), regionPointer($argc + 2);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for (name, ops) in [("regionObject", vec![Op::LooseEq, Op::LooseNotEq]),
            ("regionOrdering", vec![Op::Spaceship]), ("regionPointer", vec![Op::StrictEq, Op::StrictNotEq])] {
            let function = optimized(source, name, target);
            let text = crate::ir::print_function(&function);
            let comparisons: Vec<_> = function.instructions.iter().filter(|inst| ops.contains(&inst.op)).collect();
            assert!(comparisons.len() >= 2, "{target}: {name}: {text}");
            for comparison in comparisons {
                assert_eq!(function.values[comparison.operands[0].as_raw() as usize].ir_type,
                    crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed), "{target}: {name}: {text}");
            }
        }
    }
}
