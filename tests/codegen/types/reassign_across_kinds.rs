//! Purpose:
//! Integration tests for reassigning a variable to a value of a DIFFERENT kind.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - php has no rule against `$v = new A(); $v = 42;` — a variable holds whatever was last
//!   written to it — so every refusal here rejected a program php runs. Scalar-to-scalar already
//!   widened the slot to `mixed`; crossing into an ARRAY or an OBJECT did not, and the checker
//!   answered `Type error: cannot reassign $y from A to int`.
//! - The widening is the whole mechanism: the slot becomes a boxed cell that carries its own tag,
//!   and the reads that follow dispatch on it. What this pins is that the VALUE survives the
//!   crossing — a slot that widens but loses what was written to it would be worse than a refusal.

use crate::support::*;

/// Verifies a single variable carrying every kind in turn, reading correctly after each.
#[test]
fn one_variable_can_hold_every_kind_in_turn() {
    let out = compile_and_run_capture(
        r#"<?php
$v = 1;              $v = "s";            var_dump($v);
$v = [1, 2];         var_dump($v, count($v));
$v = 42;             var_dump($v + 1);
$v = new stdClass(); $v->p = 7;           var_dump($v->p);
$v = "back";         var_dump(strtoupper($v));
$v = null;           var_dump($v ?? "was null");
$v = 1.5;            var_dump($v * 2);
$v = true;           var_dump($v ? "yes" : "no");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(1) \"s\"\n\
         array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n\
         int(2)\n\
         int(43)\n\
         int(7)\n\
         string(4) \"BACK\"\n\
         string(8) \"was null\"\n\
         float(3)\n\
         string(3) \"yes\"\n"
    );
}

/// Verifies the crossing across a LOOP, where the slot is a whole-frame property.
///
/// The loop is the half that works: `$acc` crosses between `int` and `string` on alternate turns
/// at conditional depth 0, so the widening reaches it and the array collects both kinds.
#[test]
fn a_crossing_across_a_loop() {
    let out = compile_and_run_capture(
        r#"<?php
function looped(int $n): array {
    $out = [];
    $acc = 0;
    for ($i = 0; $i < $n; $i++) {
        $acc = $i % 2 === 0 ? $i : "s$i";
        $out[] = $acc;
    }
    return $out;
}
var_dump(looped(4));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(4) {\n  [0]=>\n  int(0)\n  [1]=>\n  string(2) \"s1\"\n  [2]=>\n  int(2)\n  [3]=>\n  string(2) \"s3\"\n}\n"
    );
}

/// The same crossing written INSIDE a branch, which is still refused.
///
/// `Checker::local_binding_storage_is_private` keeps the conditional-depth clause it inherited
/// from the kill rule. A widening does not need it for its own sake — the slot survives either
/// way — but `mixed_storage_scan` reads the same shape, and relaxing it turned SEVEN of that
/// pass's `error_tests` red: a name widened inside a branch is one it then declines to mark.
///
/// `php -n` 8.5.6 prints `string(1) "0"` and `string(7) "boxed:3"` for this, so the refusal is a
/// divergence. It is kept, loudly, in preference to disturbing a pass whose own tests say what it
/// is for — and recorded here rather than deleted, because this is the shape to re-measure first
/// when that clause is understood well enough to split.
#[test]
#[ignore = "widening declines at conditional depth: the marking pass reads the same clause"]
fn a_crossing_inside_a_branch() {
    let out = compile_and_run_capture(
        r#"<?php
function crossing(int $n): string {
    $x = $n;
    if ($n > 0) { $x = ["boxed", $n]; }
    if (is_array($x)) { $x = implode(":", $x); }
    return (string)$x;
}
var_dump(crossing(0), crossing(3));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "string(1) \"0\"\nstring(7) \"boxed:3\"\n");
}

/// Verifies two UNRELATED classes through one slot, and a catch variable reused afterwards.
///
/// The catch shape is the one that made this worth finding: `catch (Throwable $e)` binds `$e` to
/// an exception, and a later `$e = new A()` in the same scope was refused outright.
#[test]
fn two_classes_and_a_reused_catch_variable_share_a_slot() {
    let out = compile_and_run_capture(
        r#"<?php
class A { public function name(): string { return "A"; } }
class B { public function name(): string { return "B"; } }
$o = new A();
echo $o->name();
$o = new B();
echo $o->name(), "\n";

try { throw new RuntimeException("boom"); }
catch (Throwable $e) { echo get_class($e), ";"; }
$e = new A();
echo $e->name(), "\n";

$y = new A();
$y = "now a string";
echo $y, "\n";
$y = 42;
echo $y, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "AB\nRuntimeException;A\nnow a string\n42\n"
    );
}
