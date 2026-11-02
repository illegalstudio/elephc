//! Purpose:
//! Exercises numeric frame cells consumed directly by relational comparisons.
//!
//! Called from:
//! - The optimizer codegen tests through `stack_numeric`.
//!
//! Key details:
//! - Checks allocation scaling, overflow promotion, integer precision and all target emitters.

use super::*;

/// Optimized comparisons avoid per-product boxes; both modes retain the same clean result.
#[test]
fn test_stack_numeric_direct_comparison_bounds_allocations() {
    let source = r#"<?php
function comparedProducts(int $n, int $value, int $bound): int {
    $hits = 0;
    for ($i = 0; $i < $n; $i++) {
        if ($value * $value <= $bound) { $hits++; }
    }
    return $hits;
}
echo comparedProducts((int)($argc * 1000), $argc + 2, 100);
"#;
    for optimized in [false, true] {
        let large = run(source, optimized);
        let small = run(&source.replace("* 1000", "* 10"), optimized);
        assert_eq!(large.0, "1000");
        assert_eq!(small.0, "10");
        let (large_allocs, large_frees) = parse_gc_stats(&large.1);
        let (small_allocs, _) = parse_gc_stats(&small.1);
        assert_eq!(large_allocs, large_frees);
        if optimized {
            assert!(large_allocs.abs_diff(small_allocs) <= 8,
                "comparison product allocations: small={small_allocs}, large={large_allocs}");
        }
    }
}

/// Direct observations preserve exact integer ordering, overflow floats and unordered NaN results.
#[test]
fn test_stack_numeric_direct_comparison_preserves_numeric_semantics() {
    let source = r#"<?php
function productOrder(int $left, int $right, int $bound): void {
    echo (int)($left * $right < $bound), (int)($left * $right <= $bound),
        (int)($left * $right > $bound), (int)($left * $right >= $bound), '|';
}
function nanOrder(int $value): void {
    echo (int)(($value * $value) * NAN < 0), (int)(($value * $value) * NAN <= 0),
        (int)(($value * $value) * NAN > 0), (int)(($value * $value) * NAN >= 0), '|';
}
productOrder($argc + 2, 3, 10);
productOrder(9007199254740993, $argc, 9007199254740992);
productOrder(PHP_INT_MAX, $argc + 1, PHP_INT_MAX);
productOrder(PHP_INT_MIN, -$argc, PHP_INT_MAX);
nanOrder($argc);
"#;
    let expected = "1100|0011|0011|0101|0000|";
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, expected);
    }
}

/// Every supported target retains checked promotion while avoiding heap-backed product helpers.
#[test]
fn test_stack_numeric_direct_comparison_emits_all_targets() {
    let source = r#"<?php
#[Export]
function compareProduct(int $left, int $right, int $bound): bool {
    return $left * $right <= $bound;
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let assembly = super::super::checked_numeric_chain::emit_target_assembly(source, target);
        assert!(assembly.contains("stack_numeric_overflow"), "{target}");
        assert!(assembly.contains("numeric_compare_integer"), "{target}");
        assert!(!assembly.lines().any(|line| line.contains("__rt_int_mul_checked")
            && (line.contains("call ") || line.contains("bl "))), "{target}");
    }
}
