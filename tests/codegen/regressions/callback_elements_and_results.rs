//! Purpose:
//! Regression tests for callbacks over lists of containers and for values callbacks return.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `array_map()` over a list whose elements are arrays, hashes or closures hands each element to
//!   the callback as one heap pointer, the way an object element is handed over.
//! - A closure returning an element of an untyped parameter (`fn ($r) => $r["n"]`) returns it as
//!   read at run time; the syntactic default typed it `int`.
//! - A callable invoked through the runtime invoker returns its container or object with the
//!   caller's own reference, and the result cell takes that reference instead of adding one.

use crate::support::*;

/// `array_map()` maps lists of lists, of hashes and of closures, whatever the callback returns.
#[test]
fn test_array_map_over_lists_of_containers_and_closures() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function v6() { return implode(",", array_map(fn($a) => count($a), [[1], [1, 2]])); }
function v7() { return implode(",", array_map(fn(array $a) => count($a), [[1], [1, 2]])); }
function v8() { return implode(",", array_map(fn($a) => $a[0], [[5], [6, 2]])); }
function v9() { $rows = [["n" => "a"], ["n" => "b"]]; return implode(",", array_map(fn($r) => $r["n"], $rows)); }
function v11() { $rows = [[1, 2], [3]]; $r = array_map(fn($r) => array_reverse($r), $rows); return json_encode($r); }
function v12() { $fs = [fn() => 1, fn() => 2]; return implode(",", array_map(fn($f) => $f(), $fs)); }
function v13() { $rows = [[1, 2], [3]]; $r = array_map(function ($r) { $r[] = 9; return count($r); }, $rows); return implode(",", $r) . json_encode($rows); }
function v14() { $s = "x"; $rows = [["a"], ["b"]]; return implode(",", array_map(fn($r) => $r[0] . $s, $rows)); }
function run(): void {
    $out = "";
    for ($i = 0; $i < 3; $i++) {
        $out = v6() . "|" . v7() . "|" . v8() . "|" . v9() . "|" . v11() . "|" . v12() . "|" . v13() . "|" . v14();
    }
    echo $out, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1,2|1,2|5,6|a,b|[[2,1],[3]]|1,2|3,2[[1,2],[3]]|ax,bx\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A closure returning an element of an untyped parameter returns the element itself.
#[test]
fn test_closure_returning_an_element_of_an_untyped_parameter() {
    let out = compile_and_run(
        r#"<?php
function w1() { $f = fn($r) => $r["n"]; return $f(["n" => "a"]); }
function w2() { $rows = [["n" => "a"], ["n" => "b"]]; $f = fn($r) => $r["n"]; return $f($rows[1]); }
function w3() { $rows = [["n" => "a"], ["n" => "b"]]; return implode(",", array_map(fn(array $r) => $r["n"], $rows)); }
function w5() { $rows = [["n" => 1], ["n" => 2]]; return implode(",", array_map(fn($r) => $r["n"], $rows)); }
function w6() { $rows = [["n" => "a"], ["n" => "b"]]; return implode(",", array_map(function ($r) { return $r["n"]; }, $rows)); }
echo w1(), "|", w2(), "|", w3(), "|", w5(), "|", w6(), "\n";
"#,
    );
    assert_eq!(out, "a|b|a,b|1,2|a,b\n");
}

/// Containers and objects returned through the callable invoker are released with their last
/// holder: objects are destroyed exactly when php destroys them, and nothing leaks.
#[test]
fn test_invoker_returned_containers_and_objects_are_released() {
    let out = compile_and_run(
        r#"<?php
class D { public function __construct(public int $n) {} public function __destruct() { echo "[~", $this->n, "]"; } }
function i1() { $r = array_map(fn($x) => new D($x), [1, 2]); echo count($r); $r = null; echo "|"; }
function i2() { $f = fn($x) => new D($x); $g = $f; $o = call_user_func($g, 3); echo $o->n; unset($o); echo "|"; }
function i3() { $keep = new D(4); $f = fn() => $keep; $o = call_user_func($f); unset($o); echo "k"; unset($keep); unset($f); echo "|"; }
function i4() { $r = array_map(fn($x) => [$x, $x], [1, 2]); echo json_encode($r); echo "|"; }
function i5() { $keep = [new D(5)]; $r = array_map(fn($x) => $keep, [1]); unset($keep); echo count($r); $r = null; echo "|"; }
function i6() { $o = new D(6); $r = array_map(fn($x) => $x, [$o]); unset($o); echo count($r); $r = null; echo "|"; }
i1(); i2(); i3(); i4(); i5(); i6(); echo "\n";
"#,
    );
    assert_eq!(out, "2[~1][~2]|3[~3]|k[~4]|[[1,1],[2,2]]|1[~5]|1[~6]|\n");
}

/// A string cast from a `mixed` value is persisted when it is returned or handed back by a
/// builtin wrapper; it used to point into the shared scratch buffer, which the next string
/// operation overwrote.
#[test]
fn test_strings_cast_from_mixed_survive_the_return() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function t(mixed $x) { return strval($x); }
function u(mixed $x): string { return (string) $x; }
function q1() { $a = t(1); $b = t(22); $c = t("s"); $d = t(1.5); return "$a|$b|$c|$d"; }
function q2() { $a = u(7); $b = u("w"); $c = u(true); return "$a|$b|$c"; }
function q3() { $a = [1, 2]; $a[] = "x"; return json_encode(array_map("strval", $a)); }
function q4() { $a = array_map("count", [[1], [1, 2]]); return json_encode(array_map("strval", $a)); }
function run(): void {
    $out = "";
    for ($i = 0; $i < 3; $i++) {
        $out = q1() . "|" . q2() . "|" . q3() . "|" . q4();
    }
    echo $out, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1|22|s|1.5|7|w|1|[\"1\",\"2\",\"x\"]|[\"1\",\"2\"]\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
