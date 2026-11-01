//! Purpose:
//! Ownership and ordering of the interpreter's callback-taking array builtins.
//!
//! Called from:
//! - The native codegen regression harness on executable CI targets.
//!
//! Key details:
//! - The fixtures run inside `eval()`, so the interpreted builtins (not the compiled ones) answer.
//! - Each fixture runs once and ten times from one binary: a per-call leak shows up as a heap that
//!   is no longer clean, and the ten-iteration run multiplies it.
//! - Expected outputs are `php -n` 8.5's.

use crate::support::*;

/// `array_map`, `array_filter`, `usort`/`uasort`/`uksort`, `array_walk`, `array_reduce`,
/// `array_find`/`array_any`/`array_all`, `array_keys` and `implode` release what they read.
///
/// Every per-element key read, element read, callback result and callable-array receiver read was
/// kept: about nine blocks per `array_map` call over three elements, and a callable array's
/// receiver once per call. `array_reduce` keeps the initial value the caller's while releasing
/// each replaced carry, including a callback that returns its own argument.
#[test]
fn test_eval_array_callback_builtins_release_per_element_reads() {
    let source = r#"<?php
class EvalArrayCallbackTarget {
    public function by($a, $b) { return $a <=> $b; }
    public function dbl($v) { return $v * 2; }
}
eval('
$n = count($argv) > 1 ? 10 : 1;
$c = new EvalArrayCallbackTarget();
$out = [];
for ($i = 0; $i < $n; $i++) {
    $arr = [3, 1, 2];
    $keyed = ["x" => 3, "y" => 1, "z" => 2];
    $out = [];
    $out[] = implode(",", array_map(fn ($v) => $v * 2, $keyed));
    $out[] = implode(",", array_map(fn ($a, $b) => $a . $b, $arr, $arr));
    $out[] = count(array_map(null, $arr, $arr));
    $out[] = implode(",", array_map([$c, "dbl"], $arr));
    $out[] = implode(",", array_map(fn ($v) => $v, ["a", "b"]));
    $out[] = implode(",", array_filter($arr, fn ($v) => $v > 1));
    $out[] = implode(",", array_keys(array_filter($keyed, fn ($v, $k) => $k !== "y", ARRAY_FILTER_USE_BOTH)));
    $sorted = $arr; usort($sorted, fn ($a, $b) => $a <=> $b); $out[] = implode(",", $sorted);
    $sorted = $arr; usort($sorted, [$c, "by"]); $out[] = implode(",", $sorted);
    $byValue = $keyed; uasort($byValue, fn ($a, $b) => $a <=> $b); $out[] = implode(",", array_keys($byValue));
    $byKey = $keyed; uksort($byKey, fn ($a, $b) => strcmp($b, $a)); $out[] = implode(",", array_keys($byKey));
    array_walk($arr, function ($v, $k) { });
    $out[] = array_reduce($arr, fn ($carry, $v) => $carry + $v, 0);
    $out[] = array_reduce(["p", "q"], fn ($carry, $v) => $v, "s");
    $out[] = array_reduce(["p", "q"], fn ($carry, $v) => $carry, "s") . array_reduce([], fn ($carry, $v) => $carry, "e");
    $out[] = array_find($arr, fn ($v) => $v < 3) . array_find_key($keyed, fn ($v) => $v < 3);
    $out[] = (array_any($arr, fn ($v) => $v > 2) ? "y" : "n") . (array_all($arr, fn ($v) => $v > 0) ? "y" : "n");
}
echo implode("|", $out), "\n";
');
"#;
    let expected = "6,2,4|33,11,22|3|6,2,4|a,b|3,2|x,z|1,2,3|1,2,3|y,z,x|z,y,x|6|q|se|1y|yy\n";
    let runs = compile_and_run_with_heap_debug_per_argv(source, &[&[], &["ten"]]);
    for run in runs {
        assert!(run.success, "stdout={:?}\nstderr={}", run.stdout, run.stderr);
        assert_eq!(run.stdout, expected);
        assert!(run.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", run.stderr);
    }
}

/// The interpreter's `usort()` is stable and O(n log n), as php 8's sort is.
///
/// It was a bubble sort: quadratic comparator calls (over a million for 1500 elements), and the
/// kept order of equal elements was an accident of adjacent swaps.
#[test]
fn test_eval_usort_is_stable_and_n_log_n() {
    let out = compile_and_run(
        r#"<?php
echo eval('
$n = 1500;
$a = [];
for ($i = 0; $i < $n; $i++) { $a[] = ["k" => ($i * 7919) % 97, "i" => $i]; }
$calls = 0;
usort($a, function ($x, $y) use (&$calls) { $calls++; return $x["k"] <=> $y["k"]; });
$stable = true;
for ($i = 1; $i < $n; $i++) {
    if ($a[$i - 1]["k"] > $a[$i]["k"]) { $stable = false; }
    if ($a[$i - 1]["k"] === $a[$i]["k"] && $a[$i - 1]["i"] > $a[$i]["i"]) { $stable = false; }
}
$b = ["b" => 2, "a" => 2, "c" => 1, "d" => 3];
uasort($b, fn ($x, $y) => $x <=> $y);
return ($stable ? "stable" : "unstable") . " " . ($calls < 60000 ? "nlogn" : "quadratic") . " " . implode(",", array_keys($b));
'), "\n";
"#,
    );
    assert_eq!(out, "stable nlogn c,b,a,d\n");
}
