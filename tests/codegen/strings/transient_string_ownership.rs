//! Purpose:
//! Checks that transient string temporaries keep exactly one releasing owner across user calls.
//!
//! Called from:
//! - The string codegen integration tests.
//!
//! Key details:
//! - A `.` result that no longer fits concat scratch lives in a heap block stamped
//!   `CONCAT_TEMP_HEAP_KIND`; `__rt_str_persist` adopts such a block in place.
//! - Passed to a user function that may hand it back, the block used to return as an owning call
//!   result that the caller released after a store had already adopted it, so variables,
//!   elements and properties pointed at freed memory. The caller now claims it first.
//! - Allocations made after each call reuse any block freed too early, so the printed bytes
//!   expose a premature free, and heap debug reports leaks and double frees.

use crate::support::*;

/// Passthrough results stored through every user call kind keep their own bytes.
#[test]
fn transient_string_passthrough_results_survive_their_call() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function id(string $s): string { return $s; }
class Box {
    public string $v = "";
    public function pass(string $s): string { return $s; }
    public static function spass(string $s): string { return $s; }
}
$width = 69999 + $argc;
$big = str_repeat("a", $width);
$closure = function (string $s): string { return $s; };
$arrow = fn(string $s): string => $s;
$box = new Box();
$list = [];
$x1 = id($big . "1");
$x2 = $box->pass($big . "2");
$x3 = Box::spass($big . "3");
$x4 = $closure($big . "4");
$x5 = $arrow($big . "5");
$list[0] = id($big . "6");
$box->v = id($big . "7");
$list[] = $box->pass($big . "8");
$x9 = id(id($big . "9"));
$pad = [];
for ($i = 0; $i < 10; $i++) { $pad[] = str_repeat("z", $width + 3); }
echo substr($x1, -2), substr($x2, -2), substr($x3, -2), substr($x4, -2), substr($x5, -2),
    substr($list[0], -2), substr($box->v, -2), substr($list[1], -2), substr($x9, -2), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "a1a2a3a4a5a6a7a8a9\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The passthrough results stay intact with IR optimization disabled too.
#[test]
fn transient_string_passthrough_results_survive_their_call_no_ir_opt() {
    without_ir_opt(transient_string_passthrough_results_survive_their_call);
}

/// A callee that stores its argument and also returns it leaves both owners independent.
#[test]
fn transient_string_argument_stored_and_returned_by_the_callee() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Holder {
    public string $v = "";
    public function __construct(string $init) { $this->v = $init; }
    public function keep(string $s): string { $this->v = $s; return $s; }
}
$width = 69999 + $argc;
$big = str_repeat("a", $width);
$holder = new Holder($big . "c");
$kept = $holder->keep($big . "k");
$pad = [];
for ($i = 0; $i < 6; $i++) { $pad[] = str_repeat("z", $width + 3); }
echo substr($holder->v, -2), substr($kept, -2), strlen($kept), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "akak70001\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Callees summarized as maybe returning an argument release it on the paths that do not.
#[test]
fn transient_string_maybe_returned_arguments_are_released_when_not_returned() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function maybe(string $s, bool $c): string { if ($c) { return $s; } return "x"; }
function pick(string $a, string $b, bool $c): string { return $c ? $a : $b; }
$width = 69999 + $argc;
$big = str_repeat("a", $width);
$kept = [];
for ($i = 0; $i < 4; $i++) {
    echo maybe($big . "m", false);
    $kept[] = maybe($big . "k", true);
    echo strlen(pick($big . "p", $big . "q", $i % 2 === 0)), ",";
    echo maybe("small" . $i, false), maybe("small" . $i, true), ";";
}
$pad = [];
for ($i = 0; $i < 6; $i++) { $pad[] = str_repeat("z", $width + 3); }
echo "\n", substr($kept[0], -2), substr($kept[3], -2), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "x70001,xsmall0;x70001,xsmall1;x70001,xsmall2;x70001,xsmall3;\nakak\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The maybe-returned argument release also holds with IR optimization disabled.
#[test]
fn transient_string_maybe_returned_arguments_are_released_when_not_returned_no_ir_opt() {
    without_ir_opt(transient_string_maybe_returned_arguments_are_released_when_not_returned);
}

/// Echoing or measuring a large passthrough result frees the claimed block exactly once.
#[test]
fn transient_string_passthrough_results_consumed_without_a_store() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function id(string $s): string { return $s; }
$width = 69999 + $argc;
$big = str_repeat("a", $width);
for ($i = 0; $i < 4; $i++) {
    echo strlen(id($big . "e")), ",", substr(id($big . "s"), -1), ";";
}
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "70001,s;70001,s;70001,s;70001,s;\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Builtins that answer with a view into their argument keep that argument alive.
#[test]
fn transient_string_builtin_views_returned_by_user_functions_stay_valid() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function tail(string $s): string { return substr($s, 1); }
function left(string $s): string { return ltrim($s, "a"); }
function cut(string $s, bool $c): string { if ($c) { return $s; } return substr($s, 2); }
$width = 69999 + $argc;
$big = str_repeat("a", $width);
$t1 = tail($big . "T1");
$t2 = left($big . "T2");
$t3 = cut($big . "T3", false);
$t4 = left("aa" . "T4");
$pad = [];
for ($i = 0; $i < 6; $i++) { $pad[] = str_repeat("z", $width + 4); }
echo substr($t1, -2), $t2, substr($t3, -2), $t4, "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "T1T2T3T4\n");
}

/// A callee that keeps a copy of a claimed argument and then throws still frees the original.
#[test]
fn transient_string_claimed_argument_survives_a_throwing_callee() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Keeper {
    public string $v = "";
    public bool $fail = true;
    public function keep(string $s): string {
        $this->v = $s;
        if ($this->fail) { throw new Exception("k"); }
        return $s;
    }
    public function copy(string $s): string {
        $t = $s;
        if ($this->fail) { throw new Exception("c" . strlen($t)); }
        return $s;
    }
}
$width = 69999 + $argc;
$big = str_repeat("a", $width);
$keeper = new Keeper();
for ($i = 0; $i < 3; $i++) {
    try { $keeper->keep($big . "k"); } catch (Exception $e) { echo $e->getMessage(); }
    try { $keeper->copy($big . "c"); } catch (Exception $e) { echo $e->getMessage(); }
}
echo "|", substr($keeper->v, -2), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "kc70001kc70001kc70001|ak\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A claimed argument handed back through a boxed result (`mixed`, `?string`, a union) is freed.
#[test]
fn transient_string_claimed_argument_returned_through_a_boxed_result() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function asMixed(string $s): mixed { return $s; }
function asNullable(string $s): ?string { return $s; }
function asUnion(string $s): string|int { return $s; }
$width = 69999 + $argc;
$big = str_repeat("a", $width);
$kept = [];
for ($i = 0; $i < 3; $i++) {
    $m = asMixed($big . "m");
    $n = asNullable($big . "n");
    $u = asUnion($big . "u");
    echo substr($m, -1), substr($n, -1), substr($u, -1);
    $kept[] = asMixed($big . "k");
}
$pad = [];
for ($i = 0; $i < 6; $i++) { $pad[] = str_repeat("z", $width + 3); }
echo "|", substr($kept[2], -2), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "mnumnumnu|ak\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Throwing and boxed-result callees stay balanced with IR optimization disabled too.
#[test]
fn transient_string_claimed_argument_edge_cases_no_ir_opt() {
    without_ir_opt(transient_string_claimed_argument_survives_a_throwing_callee);
    without_ir_opt(transient_string_claimed_argument_returned_through_a_boxed_result);
}

/// A borrowed view of a live string (a builtin slice or a string offset) is never claimed.
///
/// Claiming one made the post-call release free the live string it points into, which the
/// trailing reads expose. Only the bytes are asserted: the copy that persisting a builtin view
/// makes before a may-return call is never released, a leak that predates the claim and is
/// tracked separately (Code Journal task b3641d28).
#[test]
fn transient_string_views_of_live_strings_are_not_claimed() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function shortOrX(string $s): string { if (strlen($s) > 0) { return "x"; } return $s; }
function boxed(string $s): mixed { return $s; }
$width = 69999 + $argc;
$big = str_repeat("a", $width) . "b";
for ($i = 0; $i < 3; $i++) {
    echo shortOrX(substr($big, 0)), shortOrX($big[0]), shortOrX(ltrim($big, "z"));
    echo strlen(boxed(substr($big, 0))), ",";
    $pad = str_repeat("z", $width + 1);
    echo strlen($big), substr($big, -2), ";";
}
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "xxx70001,70001ab;xxx70001,70001ab;xxx70001,70001ab;\n");
}

/// Views of live strings stay unclaimed with IR optimization disabled too.
#[test]
fn transient_string_views_of_live_strings_are_not_claimed_no_ir_opt() {
    without_ir_opt(transient_string_views_of_live_strings_are_not_claimed);
}
