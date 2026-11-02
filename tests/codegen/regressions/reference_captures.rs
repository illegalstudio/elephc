//! Purpose:
//! Regression tests for closures that capture variables by reference, and for values read back
//! through a reference.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A `use (&$x)` cell is owned by the frame through a hidden owner slot and shared with the
//!   closure descriptor, which takes its own reference; the last of the two frees it. It used to
//!   have no owner and leaked on every closure creation.
//! - A by-reference `foreach` value points into the array's payload. A closure capturing it keeps
//!   the element's representation instead of widening it to `mixed`.
//! - A value returned through a reference takes its own reference before the frame releases the
//!   cell, and a statically bound closure call stops reading a captured variable's slot once that
//!   variable is unset.
//! - Closures held in arrays, read back out of them, or copied by `Closure::bind` are released
//!   with their last holder, together with what they captured.

use crate::support::*;

/// By-reference captures see and make every write, whether the closure escapes, is copied,
/// rebound with `Closure::bind`, nested, or called from another callback, and none leaks.
#[test]
fn test_reference_captures_share_their_variable_without_leaking() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public $v = 1; }
function counter() { $n = 0; return function () use (&$n) { return ++$n; }; }
function c1() { $c = counter(); $d = counter(); $c(); $c(); return $c() . $d(); }
function c2() { $s = "a"; $f = function () use (&$s) { $s .= "b"; }; $f(); $f(); return $s; }
function c3() { $a = [1]; $f = function ($v) use (&$a) { $a[] = $v; }; $f(2); $f(3); return implode(",", $a); }
function c4() { $o = new Box(); $f = function () use (&$o) { $o = new Box(); $o->v = 7; }; $f(); return $o->v; }
function c5() { $t = 0; $fs = []; for ($i = 0; $i < 3; $i++) { $fs[] = function () use (&$t, $i) { $t += $i; }; } foreach ($fs as $f) { $f(); } return $t; }
function c6() { $s = "x" . mt_rand(1, 1); $f = function () use (&$s) { return $s; }; $s = "y"; return $f(); }
function c7() { $s = "x"; $f = function () use (&$s) { $s = "z"; }; $g = Closure::bind($f, null, null); $g(); unset($f); $g(); return $s; }
function c12() { $n = 1; $outer = function () use (&$n) { $inner = function () use (&$n) { $n *= 3; }; $inner(); }; $outer(); return $n; }
function c13() { $s = "k"; $f = function () use (&$s) { $s .= "m"; }; $f(); $g = $f; unset($f); $g(); return $s; }
function c14() { $n = 0; $f = function () use (&$n) { $n++; }; array_map(function () use ($f) { $f(); }, [1, 2, 3]); return $n; }
function c18() { $s = "p"; $mk = function () use (&$s) { return function () use (&$s) { $s .= "q"; }; }; $g = $mk(); $g(); $g(); return $s; }
function c21() { $s = "a"; $w = function () use (&$s) { $s = 5; }; $r = function () use (&$s) { return $s; }; $w(); return $r(); }
function c24() { $s = "a"; $r = function () use (&$s) { return $s; }; $w = function () use (&$s) { $s = 5; }; $w(); return $r(); }
function run(): void {
    $out = "";
    for ($i = 0; $i < 4; $i++) {
        $out = c1() . "|" . c2() . "|" . c3() . "|" . c4() . "|" . c5() . "|" . c6() . "|" . c7()
            . "|" . c12() . "|" . c13() . "|" . c14() . "|" . c18() . "|" . c21() . "|" . c24();
    }
    echo $out, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "31|abb|1,2,3|7|3|y|z|3|kmm|3|pqq|5|5\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A closure capturing a by-reference `foreach` value, an alias, a by-reference parameter or a
/// `global` writes the variable the others read, in the representation it already has.
#[test]
fn test_reference_captures_of_foreach_values_aliases_parameters_and_globals() {
    let out = compile_and_run(
        r#"<?php
function e1() { $arr = [1, 2, 3]; $fs = []; foreach ($arr as &$v) { $fs[] = function () use (&$v) { $v *= 10; }; } unset($v); foreach ($fs as $f) { $f(); } return implode(",", $arr); }
function e2() { $arr = [1, 2, 3]; foreach ($arr as &$v) { $f = function () use (&$v) { $v *= 10; }; $f(); } unset($v); return implode(",", $arr); }
function a1() { $b = "q"; $a = &$b; $f = function () use (&$a) { $a .= "r"; }; $f(); return $b; }
function p1() { $h = function (&$p) { $f = function () use (&$p) { $p .= "!"; }; $f(); }; $x = "P"; $h($x); return $x; }
function g1() { global $gg; $gg = "g"; $f = function () use (&$gg) { $gg .= "h"; }; $f(); return $gg; }
echo e1(), " ", e2(), " ", a1(), " ", p1(), " ", g1(), "\n";
"#,
    );
    assert_eq!(out, "10,20,30 10,20,30 qr P! gh\n");
}

/// A value read through a reference and returned outlives the frame that owned the reference,
/// and a closure keeps the cell it captured after the variable is unset and reassigned.
#[test]
fn test_values_returned_through_references_and_unset_captured_variables() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function r1() { $s = "x" . mt_rand(1, 1); $a = &$s; return $a; }
function r2() { $s = "x" . mt_rand(1, 1); $a = &$s; $s = "y" . mt_rand(1, 1); return $a; }
function r5() { $s = "x" . mt_rand(1, 1); $f = function () use (&$s) { return $s; }; return $f(); }
function r6() { $s = "x" . mt_rand(1, 1); $f = function () use (&$s) { return $s; }; $t = $f(); unset($f); $s = "z"; return $t; }
function u1() { $s = "u"; $f = function () use (&$s) { return $s; }; unset($s); $s = "w"; return $f() . $s; }
function run(): void {
    $out = "";
    for ($i = 0; $i < 4; $i++) {
        $out = r1() . " " . r2() . " " . r5() . " " . r6() . " " . u1();
    }
    echo $out, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "x1 y1 x1 x1 uw\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// An untyped closure parameter accepts a different type on a later call; the first call's
/// argument type is only the checker's guess.
#[test]
fn test_untyped_closure_parameters_accept_any_later_argument() {
    let out = compile_and_run(
        r#"<?php
function c16() { $m = null; $f = function ($x) use (&$m) { $m = $x; }; $f("str"); $f(5); return var_export($m, true); }
function c16b() { $f = function ($x) { return gettype($x); }; return $f("s") . $f(5) . $f(1.5) . $f([1]); }
function c16c() { $f = fn($x) => $x; return $f("a") . $f(2); }
echo c16(), "\n", c16b(), "\n", c16c(), "\n";
"#,
    );
    assert_eq!(out, "5\nstringintegerdoublearray\na2\n");
}

/// A closure held in an array, called through an element, copied by `Closure::bind`, or shared
/// between array slots is destroyed with its last holder, and so is the object it captured.
#[test]
fn test_closures_in_arrays_and_bound_copies_release_their_captures() {
    let out = compile_and_run(
        r#"<?php
class D { public function __construct(public string $n) {} public function __destruct() { echo "[~", $this->n, "]"; } }
function a() { $fs = []; $o = new D("a"); $fs[] = function () use ($o) { return $o->n; }; unset($o); echo $fs[0](); $fs = null; echo "|"; }
function b() { $o = new D("b"); $f = function () use ($o) { return $o->n; }; unset($o); $g = Closure::bind($f, null, null); unset($f); echo $g(); unset($g); echo "|"; }
function c() { $o = new D("c"); $fs = [function () use ($o) { return $o->n; }]; unset($o); echo $fs[0](); unset($fs); echo "|"; }
function d() { $o = new D("d"); $f = function () use ($o) { return $o->n; }; unset($o); $fs = [$f, $f]; unset($f); $fs[1] = null; echo $fs[0](); $fs = []; echo "|"; }
function e() { $o = new D("e"); $fs = []; $fs[] = function () use ($o) { return $o->n; }; unset($o); $copy = $fs; $fs = null; echo $copy[0](); unset($copy); echo "|"; }
a(); b(); c(); d(); e(); echo "\n";
"#,
    );
    assert_eq!(out, "a[~a]|b[~b]|c[~c]|d[~d]|e[~e]|\n");
}
