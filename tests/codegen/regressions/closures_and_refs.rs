//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions closures and refs, including closure default param, closure default param overridden, and for compound subtract.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests closure with a typed int default parameter: `$y = 10` is used when
/// the closure is called with only one argument.
#[test]
fn test_closure_default_param() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5);
"#,
    );
    assert_eq!(out, "15");
}

/// Tests closure with a typed int default parameter overridden by caller:
/// `$y = 10` is ignored because a second argument `20` is passed.
#[test]
fn test_closure_default_param_overridden() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5, 20);
"#,
    );
    assert_eq!(out, "25");
}

/// Tests for-loop with compound subtraction (`$i -= 3`): verifies the loop
/// iterates correctly from 10 down to 1 with step -3.
#[test]
fn test_for_compound_subtract() {
    let out = compile_and_run(
        r#"<?php
for ($i = 10; $i > 0; $i -= 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "10 7 4 1 ");
}

/// Tests for-loop with compound addition (`$i += 3`): verifies the loop
/// iterates correctly from 0 up to 9 with step +3.
#[test]
fn test_for_compound_add() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 10; $i += 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "0 3 6 9 ");
}

/// Tests for-loop with compound multiplication (`$i *= 2`): verifies the loop
/// iterates correctly doubling from 1 to 64.
#[test]
fn test_for_compound_multiply() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 100; $i *= 2) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 32 64 ");
}

/// Tests for-loop with compound left shift (`$i <<= 1`): verifies the loop
/// iterates correctly doubling from 1 to 16.
#[test]
fn test_for_compound_shift_left() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 20; $i <<= 1) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 ");
}

/// Tests closure with `use ($factor)` capturing an integer by value from the
/// enclosing scope. Verifies the captured value is used inside the closure.
#[test]
fn test_closure_use_int() {
    let out = compile_and_run(
        r#"<?php
$factor = 3;
$mul = function($x) use ($factor) { return $x * $factor; };
echo $mul(5);
"#,
    );
    assert_eq!(out, "15");
}

/// Tests closure with `use ($greeting)` capturing a string by value from the
/// enclosing scope. Verifies string concatenation inside the closure.
#[test]
fn test_closure_use_string() {
    let out = compile_and_run(
        r#"<?php
$greeting = "Hello";
$greet = function($name) use ($greeting) { return $greeting . " " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Tests closure with `use ($a, $b)` capturing two integers by value. Verifies
/// both captured variables are accessible inside the closure body.
#[test]
fn test_closure_use_multiple() {
    let out = compile_and_run(
        r#"<?php
$a = 10;
$b = 20;
$sum = function() use ($a, $b) { return $a + $b; };
echo $sum();
"#,
    );
    assert_eq!(out, "30");
}

/// Tests closure with no parameters but with `use ($name)` capturing a string
/// by value. Verifies the closure can be called with no arguments and that
/// captured variables are accessible inside the body.
#[test]
fn test_closure_use_no_params() {
    let out = compile_and_run(
        r#"<?php
$name = "World";
$greet = function() use ($name) {
    echo "Hello " . $name;
};
$greet();
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Tests recursive self-call through a by-reference capture (`use (&$g)`):
/// a factorial closure references itself via the enclosing `$g` variable.
/// Verifies `$g(5)` computes `5! = 120`.
#[test]
fn test_closure_use_by_ref_recursive_self_call() {
    let out = compile_and_run(
        r#"<?php
$g = null;
$g = function ($n) use (&$g) {
    return $n <= 1 ? 1 : $n * $g($n - 1);
};
echo $g(5);
"#,
    );
    assert_eq!(out, "120");
}

/// Tests by-reference capture (`use (&$x)`): the closure mutates the captured
/// outer variable `$x`. Verifies the mutation is visible after the call.
#[test]
fn test_closure_use_by_ref_mutates_outer_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$f = function() use (&$x) { $x = 2; };
$f();
echo $x;
"#,
    );
    assert_eq!(out, "2");
}

/// Regression for #306: by-reference captures must observe later outer mutations.
#[test]
fn test_closure_use_by_ref_observes_later_outer_mutation() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$f = function () use (&$x) {
    echo $x;
};
$x = 2;
$f();
"#,
    );
    assert_eq!(out, "2");
}

/// Regression for #304: by-reference captures stored in arrays must survive the
/// defining function scope and observe loop updates made after the closure is created.
#[test]
fn test_closure_use_by_ref_array_escape_survives_function_scope() {
    let out = compile_and_run(
        r#"<?php
function make() {
    $fns = [];
    for ($i = 0; $i < 1; $i++) {
        $fns[] = function() use (&$i) {
            echo $i;
        };
    }
    return $fns;
}

$fns = make();
$fns[0]();
"#,
    );
    assert_eq!(out, "1");
}

/// Regression for #318: a by-reference captured array accepts post-increment on an element.
#[test]
fn test_closure_use_by_ref_array_element_post_increment() {
    let out = compile_and_run(
        r#"<?php
$a = ["n" => 1];
$f = function () use (&$a) { $a["n"]++; };
$f();
echo $a["n"];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies a hash write through a *reference-bound* local survives a table reallocation.
///
/// `lower_inst::hashes::source_load_local_slot` recognized only `Op::LoadLocal`, while its
/// indexed-array twin (`lower_inst::arrays::source_load_local_slot`) also recognizes
/// `Op::LoadRefCell`. So `$r = &$a; $r[$k] = $v;` found no destination slot and silently
/// discarded the table pointer `__rt_hash_set` hands back — which is a *new* pointer whenever
/// the table rehashes past its load factor or is COW-split. The stale pointer then kept being
/// read: building a 41-key hash through a ref reported a garbage count (a wild read), and the
/// same shape on an object property lost 37 of the 41 entries.
///
/// Three or four keys are not enough to trigger it — the hash must actually grow — so this
/// test deliberately crosses the rehash threshold.
#[test]
fn test_hash_write_through_ref_bound_local_survives_rehash() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    public array $v = [];
}
$a = ["seed" => 0];
$r = &$a;
for ($i = 0; $i < 40; $i++) {
    $r["k" . $i] = $i;
}
echo count($a), "\n";

$o = new Holder();
$o->v["seed"] = 0;
$p = &$o->v;
for ($i = 0; $i < 40; $i++) {
    $p["k" . $i] = $i;
}
echo count($o->v), "\n";
echo $o->v["k39"], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "41\n41\n39\n");
}
