//! Purpose:
//! Pins leaks reported in open issues that are fixed on main, one fixture per issue, each asserting a clean heap.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture is the issue's own reproduction, trimmed; stdout was captured from PHP 8.5 and heap-debug must report no live blocks.

use super::*;

/// `print_r($value, true)` leaked one captured-string block per call when the result was
/// reassigned in a loop. Pins correct output and a clean heap across repeated captures.
/// Regression for #628.
#[test]
fn test_issue_628_print_r_return_mode_releases_each_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$value = ["a" => 1, "b" => [2, 3]];
$rendered = "";
for ($i = 0; $i < 10; $i++) {
    $rendered = print_r($value, true);
}
echo strlen($rendered), "\n";
"#,
    );
    assert_eq!(out.stdout, "103\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// By-reference `foreach` writing strings into a heterogeneous (boxed mixed) array leaked one
/// block per written element. Pins the written values and a clean heap.
/// Regression for #652.
#[test]
fn test_issue_652_by_ref_foreach_string_writes_into_mixed_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["a", 1, 2];
foreach ($a as &$v) { $v = $v . "!"; }
unset($v);
echo implode(",", $a), "\n";
"#,
    );
    assert_eq!(out.stdout, "a!,1!,2!\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// By-reference `foreach` over a static array property leaked three blocks while producing
/// the right values. Pins the mutation of `C::$x` and a clean heap.
/// Regression for #653.
#[test]
fn test_issue_653_by_ref_foreach_over_static_array_property() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public static array $x = [1, 2]; }
foreach (C::$x as &$v) { $v = $v * 2; }
unset($v);
echo implode(",", C::$x), "\n";
"#,
    );
    assert_eq!(out.stdout, "2,4\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A fresh array passed to a callee that may return its parameter leaked on the non-aliasing
/// return path. Pins that twenty such calls leave the heap clean.
/// Regression for #665.
#[test]
fn test_issue_665_conditional_return_releases_fresh_container_arg() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function maybe($x, $c) { if ($c == 1) { return $x; } return 7; }
$s = 0;
for ($i = 0; $i < 20; $i++) { $r = maybe([$i], 0); $s = $s + 1; }
echo $s, "\n";
"#,
    );
    assert_eq!(out.stdout, "20\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An owned string local above ~64KB was never released when its function returned, leaving
/// the whole buffer live at exit. This pins a clean heap for repeated 100KB locals.
/// Regression for #774.
#[test]
fn test_issue_774_large_string_local_released_at_scope_exit() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(int $n): int {
    $a = str_repeat("x", 100000 + $n);
    $t = strlen($a);
    return $t;
}
echo f(3), "\n";
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $sum += f($i);
}
echo $sum, "\n";
"#,
    );
    assert_eq!(out.stdout, "100003\n1000045\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A top-level string reached through `global` in another function lived in program-global
/// storage that was never released at exit (48 bytes). This pins a clean heap.
/// Regression for #786.
#[test]
fn test_issue_786_global_alias_string_released_at_exit() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function q() { global $a; echo $a, "\n"; }
$a = "hello" . $argc;
q();
"#,
    );
    assert_eq!(out.stdout, "hello1\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Closure and dynamic-callable calls leaked one block per string or mixed-bound argument;
/// this pins a clean heap across 1000 iterations of each argument shape.
/// Regression for #924.
#[test]
fn test_issue_924_closure_call_args_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public int $v = 1; }
function run(int $n): int {
    $t = 0;
    $two = function ($a, $b) { return $a + $b; };
    $four = function ($a, $b, $c, $d) { return $a + $b + $c + $d; };
    $strs = function (string $a, string $b) { return strlen($a) + strlen($b); };
    $mixed = function (mixed $a, mixed $b) { return $a + $b; };
    $objstr = function (mixed $p, string $s) { return $p->v + strlen($s); };
    $typedobj = function (P $p, string $s) { return $p->v + strlen($s); };
    $arrm = function (mixed $a, mixed $b) { return count($a) + $b; };
    $p = new P();
    for ($i = 0; $i < $n; $i++) {
        $s = "x" . $i;
        $t += $two(1, 2);
        $t += $four(1, 2, 3, 4);
        $t += call_user_func($two, 1, 2);
        $t += $strs($s, "yz");
        $t += $mixed(1, 2);
        $t += $objstr($p, $s);
        $t += $typedobj($p, $s);
        $t += $arrm(["k" => $s], 2);
    }
    return $t;
}
echo run(1000), "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "37670\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// When __clone() threw, the shallow copy was never released; this pins a clean heap after
/// 200 failed clones of a plain class and of a class with heap-backed properties.
/// Regression for #946.
#[test]
fn test_issue_946_failed_clone_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public function __clone(): void { throw new Error("no"); } }
class D {
    public array $items;
    public string $name;
    public function __construct(string $n) {
        $this->name = $n . "-" . strlen($n);
        $this->items = [$n, $n . "x", [1, 2, 3]];
    }
    public function __clone(): void { throw new RuntimeException("no clone"); }
}
$c = new C();
$d = new D("orig");
$caught = 0;
for ($i = 0; $i < 100; $i++) {
    try { $x = clone $c; } catch (Error $e) { $caught++; }
    try { $y = clone $d; } catch (RuntimeException $e) { $caught++; }
}
echo "caught ", $caught, "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "caught 200\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A mixed function returning its mixed parameter from a conditional return corrupted an
/// object payload's refcount under --heap-debug. Pins correct output and a clean heap.
/// Regression for #992.
#[test]
fn test_issue_992_conditional_param_return_keeps_object_refcount() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Tag { public string $s = "tag"; }

function pick(int $i): mixed {
    if ($i > 100) { return "never"; }
    return new Tag();
}

function ident(mixed $value): mixed {
    if (is_object($value)) { return $value; }
    return $value;
}

for ($i = 0; $i < 5; $i++) {
    $tmp = pick($i);
    $value = ident($tmp);
}
echo $value->s, "\n";
echo "ok\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "tag\nok\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// array_splice() with a replacement holding refcounted elements leaked one allocation per
/// call. Pins correct results and a clean heap, top level and inside a function.
/// Regression for #1037.
#[test]
fn test_issue_1037_array_splice_refcounted_replacement_is_balanced() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 200; $i++) {
    $n = [[1], [2]];
    $m = [[3]];
    array_splice($n, 0, 1, $m);
}
echo count($n), " ", json_encode($n), "\n";
function run(int $k): array {
    $a = [[1], [2, $k]];
    $b = [[3, $k], [4]];
    $removed = array_splice($a, 1, 1, $b);
    return [$a, $removed];
}
for ($i = 0; $i < 200; $i++) { [$x, $r] = run($i); }
echo json_encode($x), " ", json_encode($r), "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "2 [[3],[2]]\n[[1],[3,199],[4]] [[2,199]]\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Calling a callable resolved from a runtime string (named, zero-arg, call_user_func) leaked
/// blocks on every call. Pins correct sums and a clean heap.
/// Regression for #1047.
#[test]
fn test_issue_1047_runtime_string_callable_call_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function g($a = 1, $b = 2) { return $a + $b; }
function nm(): string { return "g"; }
$s = 0;
for ($i = 0; $i < 50; $i++) {
    $f = nm();
    $s += $f(b: 8);
    $s += $f();
    $s += call_user_func($f, 3);
}
echo $s, "\n";
echo "done\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "850\ndone\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Promoting [] to a hash inside a function and then sort()/rsort()-ing it hung or reported a
/// bad refcount. Pins correct results in a function, method and closure with a clean heap.
/// Regression for #1098.
#[test]
fn test_issue_1098_promoted_hash_in_function_survives_reindexing_sort() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(int $n): int {
    $h = []; $h["b"] = $n; $h["a"] = 1;
    rsort($h);
    return count($h) * 100 + $h[0];
}
function g(): int { $h = []; $h["b"] = 2; $h["a"] = 3; sort($h); return $h[0]; }
class K {
    public function m(): int { $h = []; $h["b"] = 2; $h["a"] = 1; rsort($h); return count($h); }
}
$c = function (): int { $h = []; $h["b"] = 2; rsort($h); return count($h); };
$t = 0;
for ($i = 0; $i < 20; $i++) { $t += f($i); }
echo $t, "\n";
echo g(), "\n";
echo (new K())->m(), "\n";
echo $c(), "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "4191\n2\n2\n1\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
