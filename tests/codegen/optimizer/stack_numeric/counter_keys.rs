//! Purpose:
//! Exercises numeric frame cells consumed as array keys, and ordering comparisons of inline
//! scalars that no longer box their operands.
//!
//! Called from:
//! - The optimizer codegen tests through `stack_numeric`.
//!
//! Key details:
//! - Checks allocation scaling, PHP key and ordering semantics, and all target emitters.

use super::*;

/// An insertion sort keyed by `int|float` counters (`$j + 1`, `$j--`).
const INSERTION_SORT: &str = r#"<?php
function sortCounter(int $n): int {
    $arr = [];
    $seed = 12345;
    for ($i = 0; $i < $n; $i++) { $seed = ($seed * 1103515245 + 12345) % 2147483648; $arr[] = $seed % 1000; }
    for ($i = 1; $i < $n; $i++) {
        $key = $arr[$i];
        $j = $i - 1;
        while ($j >= 0 && $arr[$j] > $key) {
            $arr[$j + 1] = $arr[$j];
            $j--;
        }
        $arr[$j + 1] = $key;
    }
    $sorted = 1;
    for ($i = 1; $i < $n; $i++) { if ($arr[$i - 1] > $arr[$i]) { $sorted = 0; } }
    return $sorted * 1000000 + $arr[0] * 1000 + $arr[$n - 1];
}
echo sortCounter($argc * 300);
"#;

/// Counter keys and their element comparisons stay in frame cells: the allocation count does
/// not grow with the number of inner-loop iterations.
#[test]
fn test_stack_numeric_counter_keys_bound_allocations() {
    for optimized in [false, true] {
        let large = run(INSERTION_SORT, optimized);
        let small = run(&INSERTION_SORT.replace("* 300", "* 30"), optimized);
        assert!(large.0.starts_with('1'), "the sort must order the array: {}", large.0);
        assert!(small.0.starts_with('1'), "the sort must order the array: {}", small.0);
        let (large_allocs, large_frees) = parse_gc_stats(&large.1);
        let (small_allocs, _) = parse_gc_stats(&small.1);
        assert_eq!(large_allocs, large_frees);
        if optimized {
            assert!(large_allocs.abs_diff(small_allocs) <= 16,
                "counter-key allocations: small={small_allocs}, large={large_allocs}");
        }
    }
}

/// Inline `?int` ordering, float counter keys and a negative counter key keep PHP's results.
#[test]
fn test_stack_numeric_counter_keys_preserve_php_semantics() {
    let source = r#"<?php
function cmpNullable(int $n): string {
    $arr = [5, 1, 9];
    $out = '';
    for ($i = 0; $i < $n; $i++) {
        $k = $arr[$i];
        $out .= ($k > 4 ? 'g' : 'l') . ($k < 2.5 ? 'f' : 'F') . ($k >= true ? 't' : 'T') . (4 <= $k ? 'r' : 'R') . '|';
    }
    return $out;
}
function floatKeys(int $n): string {
    $arr = [10, 20, 30, 40];
    $out = '';
    for ($j = $n + 0.5; $j >= 0; $j--) {
        $out .= $arr[$j] . ',';
    }
    return $out;
}
function shiftRight(int $n): string {
    $arr = [];
    for ($i = 0; $i < $n; $i++) { $arr[] = $i * 10; }
    for ($j = $n - 2; $j >= 0; $j--) { $arr[$j + 1] = $arr[$j]; }
    $arr[-1 + $n * 0] = 99;
    return json_encode($arr);
}
echo cmpNullable($argc + 3), "\n", floatKeys($argc + 2), "\n", shiftRight($argc + 3), "\n";
"#;
    let expected = "gFtr|lftR|gFtr|lfTR|\n40,30,20,10,\n{\"0\":0,\"1\":0,\"2\":10,\"3\":20,\"-1\":99}\n";
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, expected);
    }
}

/// Every target keys the shift with frame cells and compares an inline `?int` without boxing.
#[test]
fn test_stack_numeric_counter_keys_emit_all_targets() {
    let source = r#"<?php
#[Export]
function shiftLast(int $n): int {
    $arr = [];
    for ($i = 0; $i < $n; $i++) { $arr[] = $i; }
    for ($j = $n - 2; $j >= 0; $j--) { $arr[$j + 1] = $arr[$j]; }
    return count($arr);
}
#[Export]
function aboveFirst(int $n): bool {
    $arr = [$n, 2];
    $key = $arr[$n];
    return $arr[1] > $key;
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let assembly = super::super::checked_numeric_chain::emit_target_assembly(source, target);
        let exported = exported_functions(&assembly);
        assert!(exported.contains("stack_numeric_overflow"), "{target}");
        for (op, helper) in [
            ("op=str_inc_dec", "__rt_mixed_inc_dec"),
            ("op=mixed_numeric_binop", "__rt_mixed_numeric_add"),
            ("op=php_rel_cmp", "__rt_mixed_from_value"),
        ] {
            assert!(exported.contains(op), "{target}: no {op} region");
            assert!(!op_calls(&exported, op, helper), "{target}: {op} still calls {helper}");
        }
    }
}

/// Returns the assembly of the two exported fixture functions, without runtime helpers or
/// synthetic initializers that box for their own reasons.
fn exported_functions(assembly: &str) -> String {
    let mut keep = false;
    let mut out = String::new();
    for line in assembly.lines() {
        if line.contains("@fn name=") {
            keep = line.contains("@fn name=shiftLast ") || line.contains("@fn name=aboveFirst ");
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Returns whether any instruction lowered for a source op marked `op` calls `helper`.
fn op_calls(assembly: &str, op: &str, helper: &str) -> bool {
    let mut current = false;
    for line in assembly.lines() {
        if line.contains("@src ") {
            current = line.contains(op);
        } else if current && line.contains(helper) && (line.contains("call ") || line.contains("bl ")) {
            return true;
        }
    }
    false
}
