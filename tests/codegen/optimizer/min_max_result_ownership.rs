//! Purpose:
//! Regression coverage for how a variadic `min()` / `max()` result is stored: without a
//! Mixed cell when it is only read, and as an independent value when it escapes.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - A runtime-typed selection whose result is only read in its block copies a numeric
//!   winner into a frame cell, so `$s += max($a, $b)` with `mixed` operands allocates nothing.
//! - The by-value assignment of a numeric selection (`$r = max($i, $f)`) reads that cell
//!   instead of boxing the selection first.
//! - An escaping result retains the winning operand's own heap cell instead of cloning it.
//!   The value must still behave like a copy: later writes to the operand variable, through
//!   a reference or between two call arguments, never show through the result.
//! - Expected output comes from PHP 8.5; `ELEPHC_PHP_CHECK=1` re-runs the oracle.

use super::min_max_selection::{function_assembly, php_oracle, run};
use super::*;

/// Results that outlive the call while their operand variable is modified afterwards.
const ESCAPING_RESULTS: &str = r#"<?php
function two(mixed $first, mixed $second): string { return $first . "|" . $second; }
function escArray(mixed $a, mixed $b): string { $list = []; $list[] = max($a, $b); $a .= "!"; return $list[0] . " " . $a; }
function escInc(mixed $a, mixed $b): string { $list = [max($a, $b)]; $a++; return $list[0] . " " . $a; }
function escMin(mixed $a, mixed $b): string { $list = [min($a, $b), max($a, $b)]; $a = $a . "%"; $b .= "@"; return $list[0] . " " . $list[1] . " " . $a . " " . $b; }
function argOrder(mixed $p): string { return two(max($p, "a"), $p .= "+") . " " . $p; }
function argAssign(mixed $p): string { return two(max($p, "a"), $p = "reassigned") . " " . $p; }
function argAdd(mixed $q): string { return two(max($q, 1), $q += 5) . " " . $q; }
function argInc(mixed $q): string { return two(max($q, 1), ++$q) . " " . $q; }
function retKeep(mixed $a, mixed $b): mixed { return max($a, $b); }
function callerKeeps(mixed $v): string { $k = [retKeep($v, 0)]; $v += 100; return $k[0] . " " . $v; }
function viaRef(mixed &$a, mixed $b): mixed { $m = max($a, $b); $a = "changed"; return $m; }
function refHeld(mixed &$a, mixed $b): string { $h = [max($a, $b)]; $a .= "~"; return $h[0] . " " . $a; }
function three(mixed $a, mixed $b, mixed $c): string { $h = [max($a, $b, $c), min($a, $b, $c)]; $a .= "1"; $b .= "2"; $c .= "3"; return $h[0] . " " . $h[1] . " " . $a . $b . $c; }
function reused(mixed $a, mixed $b): string { $out = ""; for ($i = 0; $i < 3; $i++) { $m = max($a, $b); $a .= "x"; $out .= $m . ","; } return $out . $a; }
echo escArray("pear" . $argc, "apple"), "\n";
echo escInc(5 * $argc, 3), "\n";
echo escInc(5.5 * $argc, 3), "\n";
echo escMin("k" . $argc, "j"), "\n";
echo argOrder("m" . $argc), "\n";
echo argAssign("m" . $argc), "\n";
echo argAdd(10 * $argc), "\n";
echo argInc(10 * $argc), "\n";
echo callerKeeps(7 * $argc), "\n";
$r = "zz" . $argc; echo viaRef($r, "aa"), " ", $r, "\n";
$r = "yy" . $argc; echo refHeld($r, "aa"), " ", $r, "\n";
echo three("b" . $argc, "c", "a"), "\n";
echo reused("q" . $argc, "p"), "\n";
"#;

/// PHP 8.5 output of `ESCAPING_RESULTS`.
const ESCAPING_RESULTS_OUTPUT: &str = "pear1 pear1!\n\
5 6\n\
5.5 6.5\n\
j k1 k1% j@\n\
m1|m1+ m1+\n\
m1|reassigned reassigned\n\
10|15 15\n\
10|11 11\n\
7 107\n\
zz1 changed\n\
yy1 yy1~ yy1~\n\
c a b11c2a3\n\
q1,q1x,q1xx,q1xxx\n\
";

/// A retained winner still behaves like a copy of the argument in both optimizer modes.
#[test]
fn test_min_max_escaping_result_is_independent_of_later_operand_writes() {
    php_oracle(ESCAPING_RESULTS, ESCAPING_RESULTS_OUTPUT);
    for optimized in [true, false] {
        assert_eq!(run(ESCAPING_RESULTS, optimized), ESCAPING_RESULTS_OUTPUT, "optimized={optimized}");
    }
}

/// Runtime-typed numbers that are only read, and the assignment of a typed selection, cost
/// no allocation per call once the optimizer runs.
#[test]
fn test_min_max_read_only_results_allocate_nothing_per_call() {
    let source = r#"<?php
function work(mixed $a, mixed $b, int $n): mixed {
    $s = 0;
    for ($i = 0; $i < $n; $i++) { $s += max($a, $b); }
    return $s;
}
function below(mixed $a, mixed $b, int $n): int {
    $hits = 0;
    for ($i = 0; $i < $n; $i++) { if (min($a, $b) < 3) { $hits++; } }
    return $hits;
}
function assign(int $i, float $f, int $n): float {
    $sum = 0.0;
    for ($k = 0; $k < $n; $k++) { $r = max($i, $f); $sum += $r; }
    return $sum;
}
function texts(mixed $a, mixed $b, int $n): int {
    $len = 0;
    for ($k = 0; $k < $n; $k++) { $len += strlen((string) max($a, $b)); }
    return $len;
}
function bounded(int $allocations): string { return $allocations <= 20 ? "bounded" : "allocations=" . $allocations; }
$n = 1000 * $argc;
$start = memory_get_allocations(); $value = work(3, 2, $n); $end = memory_get_allocations();
echo "work ", $value, " ", bounded($end - $start), "\n";
$start = memory_get_allocations(); $value = below(2.5, 2.25, $n); $end = memory_get_allocations();
echo "floats ", $value, " ", bounded($end - $start), "\n";
$start = memory_get_allocations(); $value = below(3, 2, $n); $end = memory_get_allocations();
echo "below ", $value, " ", bounded($end - $start), "\n";
$start = memory_get_allocations(); $value = assign(3, 2.5, $n); $end = memory_get_allocations();
echo "assign ", $value, " ", bounded($end - $start), "\n";
echo "texts ", texts("abc", "abd", $n), " ", work("7", 2, 10), "\n";
"#;
    let expected = "work 3000 bounded\nfloats 1000 bounded\nbelow 1000 bounded\nassign 3000 bounded\ntexts 3000 70\n";
    assert_eq!(run(source, true), expected);
    let plain = run(source, false);
    for line in ["work 3000 ", "floats 1000 ", "below 1000 ", "assign 3000 ", "texts 3000 70\n"] {
        assert!(plain.contains(line), "{line}: {plain}");
    }
}

/// Every target fills the guarded frame cell for a read-only runtime-typed selection, hands
/// an escaping one straight to the ownership helper, and ships that helper in its runtime.
#[test]
fn test_min_max_result_storage_emits_on_every_target() {
    let source = r#"<?php
function work(mixed $a, mixed $b, int $n): mixed {
    $s = 0;
    for ($i = 0; $i < $n; $i++) { $s += max($a, $b); }
    return $s;
}
function keep(mixed $a, mixed $b): mixed { return max($a, $b); }
echo work($argc, 2, 3), keep($argc, 2);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let work = function_assembly(source, name, "work");
        assert!(work.contains("guarded_min_max_numeric"), "{name}: {work}");
        assert!(work.contains("__rt_mixed_min_max_own"), "{name}: {work}");
        let keep = function_assembly(source, name, "keep");
        assert!(keep.contains("__rt_mixed_min_max_own"), "{name}: {keep}");
        assert!(!keep.contains("guarded_min_max_numeric"), "{name}: {keep}");
        let target = Target::parse(name).expect("supported target");
        let runtime = elephc::codegen::generate_runtime(8_388_608, target);
        assert!(runtime.contains("__rt_mixed_min_max_own:"), "{name} runtime lacks the ownership helper");
    }
}
