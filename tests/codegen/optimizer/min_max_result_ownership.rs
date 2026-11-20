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
//! - An escaping result is a copy of the winner, never the retained operand cell. Later
//!   writes to the operand variable never show through the result: not between two call
//!   arguments, not in a loop, and not through a reference, which mutates the variable's
//!   cell in place (`$kept = [max($ref, $b)]; $ref[] = 1;` once changed `$kept`).
//! - The array fixture picks winners that are also PHP's result on a tie, because the
//!   generic comparison does not order two arrays yet; it tests value independence only.
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

/// A stored scalar or string result keeps its value in both optimizer modes.
#[test]
fn test_min_max_escaping_result_is_independent_of_later_operand_writes() {
    php_oracle(ESCAPING_RESULTS, ESCAPING_RESULTS_OUTPUT);
    for optimized in [true, false] {
        assert_eq!(run(ESCAPING_RESULTS, optimized), ESCAPING_RESULTS_OUTPUT, "optimized={optimized}");
    }
}

/// Array and string results whose operand variable, or the result itself, is modified in
/// place afterwards, including through a by-reference parameter and a by-reference call.
const CONTAINER_RESULTS: &str = r#"<?php
function j(mixed $v): string { return json_encode($v); }
function e(): mixed { return []; }
// result stored, then the winning operand variable is modified in place
function appendAfter(mixed $a, mixed $b): string { $list = []; $list[] = max($a, $b); $a[] = 9; return j($list[0]) . " " . j($a); }
function setAfter(mixed $a, mixed $b): string { $list = [max($a, $b)]; $a[0] = "changed"; return j($list[0]) . " " . j($a); }
function nestedAfter(mixed $a, mixed $b): string { $list = [max($a, $b)]; $a["k"]["j"] = 5; return j($list[0]) . " " . j($a); }
function minAfter(mixed $a, mixed $b): string { $list = [min($a, $b)]; $b[] = 7; return j($list[0]) . " " . j($b); }
function threeAfter(mixed $a, mixed $b, mixed $c): string { $list = [max($a, $b, $c)]; $c[] = 1; $b[] = 2; $a[] = 3; return j($list[0]) . " " . j($a) . j($b) . j($c); }
// the stored result is modified in place, the operand must not change
function resultModified(mixed $a, mixed $b): string { $list = [max($a, $b)]; $list[0][] = 9; return j($list[0]) . " " . j($a); }
function resultNested(mixed $a, mixed $b): string { $h = ["r" => max($a, $b)]; $h["r"][0] = "x"; return j($h) . " " . j($a); }
// returned to the caller, which then modifies its own variable
function pick(mixed $a, mixed $b): mixed { return max($a, $b); }
function callerModifies(mixed $v): string { $kept = [pick($v, e())]; $v[] = 100; return j($kept[0]) . " " . j($v); }
function callerModifiesResult(mixed $v): string { $kept = [pick($v, e())]; $kept[0][] = 100; return j($kept[0]) . " " . j($v); }
// between two call arguments
function two(mixed $first, mixed $second): string { return j($first) . "|" . j($second); }
function argThenAppend(mixed $a): string { return two(max($a, e()), $a[] = 4) . " " . j($a); }
function argThenSet(mixed $a): string { return two(max($a, e()), $a[0] = "w") . " " . j($a); }
// through a reference
function viaRef(mixed &$a, mixed $b): string { $list = [max($a, $b)]; $a[] = "ref"; return j($list[0]); }
// through a by-reference call that appends to the caller's variable
function addTo(mixed &$x): void { $x[] = "byref"; }
function byRefCall(mixed $a, mixed $b): string { $list = [max($a, $b)]; addTo($a); return j($list[0]) . " " . j($a); }
function refScalar(mixed &$a, mixed $b): string { $list = [max($a, $b), min($a, $b)]; $a = "zz"; return j($list); }
// string offsets on a held winner
function strOffset(mixed $a, mixed $b): string { $list = [max($a, $b)]; $a[0] = "Z"; return j($list[0]) . " " . j($a); }
// loop: the same variable wins repeatedly while it grows
function growing(mixed $a): string { $seen = []; for ($i = 0; $i < 3; $i++) { $seen[] = max($a, e()); $a[] = $i; } return j($seen) . " " . j($a); }
// operands that are elements of one array
function elements(mixed $arr): string { $h = [max($arr[0], $arr[1])]; $arr[0][] = "e"; $arr[1][] = "f"; return j($h[0]) . " " . j($arr); }
// static and property holders
class Box { public mixed $v; public function __construct(mixed $v) { $this->v = $v; } public function grab(mixed $o): mixed { return max($this->v, $o); } }
function prop(mixed $v): string { $box = new Box($v); $h = [$box->grab([])]; $box->v[] = "p"; return j($h[0]) . " " . j($box->v); }
function propResult(mixed $v): string { $box = new Box($v); $h = [$box->grab([])]; $h[0][] = "q"; return j($h[0]) . " " . j($box->v); }
$base = [1, 2, $argc];
echo appendAfter($base, [1]), "\n";
echo setAfter($base, [1]), "\n";
echo nestedAfter(["k" => ["j" => 1], "z" => $argc], [1]), "\n";
echo minAfter($base, [1, $argc]), "\n";
echo threeAfter($base, [1], [2, 2]), "\n";
echo resultModified($base, [1]), "\n";
echo resultNested($base, [1]), "\n";
echo callerModifies($base), "\n";
echo callerModifiesResult($base), "\n";
echo argThenAppend($base), "\n";
echo argThenSet($base), "\n";
$r = [5, $argc]; echo viaRef($r, [1]), " ", j($r), "\n";
echo byRefCall($base, [1]), "\n";
$q = "m" . $argc; echo refScalar($q, "a"), " ", j($q), "\n";
echo strOffset("abc" . $argc, "abb"), "\n";
echo growing([$argc]), "\n";
echo elements([[1, $argc], [1]]), "\n";
echo prop($base), "\n";
echo propResult($base), "\n";
echo j($base), "\n";
"#;

/// PHP 8.5 output of `CONTAINER_RESULTS`.
const CONTAINER_RESULTS_OUTPUT: &str = "[1,2,1] [1,2,1,9]\n\
[1,2,1] [\"changed\",2,1]\n\
{\"k\":{\"j\":1},\"z\":1} {\"k\":{\"j\":5},\"z\":1}\n\
[1,1] [1,1,7]\n\
[1,2,1] [1,2,1,3][1,2][2,2,1]\n\
[1,2,1,9] [1,2,1]\n\
{\"r\":[\"x\",2,1]} [1,2,1]\n\
[1,2,1] [1,2,1,100]\n\
[1,2,1,100] [1,2,1]\n\
[1,2,1]|4 [1,2,1,4]\n\
[1,2,1]|\"w\" [\"w\",2,1]\n\
[5,1] [5,1,\"ref\"]\n\
[1,2,1] [1,2,1,\"byref\"]\n\
[\"m1\",\"a\"] \"zz\"\n\
\"abc1\" \"Zbc1\"\n\
[[1],[1,0],[1,0,1]] [1,0,1,2]\n\
[1,1] [[1,1,\"e\"],[1,\"f\"]]\n\
[1,2,1] [1,2,1,\"p\"]\n\
[1,2,1,\"q\"] [1,2,1]\n\
[1,2,1]\n\
";

/// A stored array result is independent of its operand variable in both directions, also
/// when that variable is written through a reference.
#[test]
fn test_min_max_escaping_container_result_is_a_copy() {
    php_oracle(CONTAINER_RESULTS, CONTAINER_RESULTS_OUTPUT);
    for optimized in [true, false] {
        assert_eq!(run(CONTAINER_RESULTS, optimized), CONTAINER_RESULTS_OUTPUT, "optimized={optimized}");
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

/// Every target fills the guarded frame cell for a read-only runtime-typed selection and
/// clones the winner of an escaping one.
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
        let keep = function_assembly(source, name, "keep");
        assert!(!keep.contains("guarded_min_max_numeric"), "{name}: {keep}");
        let selection = keep.split("__rt_mixed_min_max_select").nth(1).expect("selection call");
        assert!(selection.contains("__rt_mixed_clone"), "{name}: {keep}");
    }
}
