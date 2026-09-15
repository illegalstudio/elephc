//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow branches and loops, including if true, if false, and if else.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that if (true) executes the branch.
#[test]
fn test_if_true() {
    let out = compile_and_run("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(out, "yes");
}

/// Verifies that if (false) skips the branch, producing no output.
#[test]
fn test_if_false() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"yes\"; }");
    assert_eq!(out, "");
}

/// Verifies if/else selects the else branch when the condition is false.
#[test]
fn test_if_else() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"a\"; } else { echo \"b\"; }");
    assert_eq!(out, "b");
}

/// Verifies elseif chain takes the second branch when first condition is false.
#[test]
fn test_if_elseif_else() {
    let out = compile_and_run(
        "<?php $x = 2; if ($x == 1) { echo \"one\"; } elseif ($x == 2) { echo \"two\"; } else { echo \"other\"; }",
    );
    assert_eq!(out, "two");
}

/// Verifies elseif/else falls through to the else branch when all conditions are false.
#[test]
fn test_if_else_falls_through() {
    let out = compile_and_run(
        "<?php $x = 99; if ($x == 1) { echo \"a\"; } elseif ($x == 2) { echo \"b\"; } else { echo \"c\"; }",
    );
    assert_eq!(out, "c");
}

// --- while ---

/// Verifies a basic while loop iterates 5 times and prints 01234.
#[test]
fn test_while_loop() {
    let out = compile_and_run("<?php $i = 0; while ($i < 5) { echo $i; $i = $i + 1; }");
    assert_eq!(out, "01234");
}

/// Verifies while (false) body never executes.
#[test]
fn test_while_zero_iterations() {
    let out = compile_and_run("<?php while (0) { echo \"no\"; }");
    assert_eq!(out, "");
}

/// Verifies break exits the while loop when $i reaches 3, producing 012.
#[test]
fn test_while_break() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 10) { if ($i == 3) { break; } echo $i; $i = $i + 1; }",
    );
    assert_eq!(out, "012");
}

/// Verifies continue skips to the next iteration, skipping the echo at $i==3.
#[test]
fn test_while_continue() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 5) { $i = $i + 1; if ($i == 3) { continue; } echo $i; }",
    );
    assert_eq!(out, "1245");
}

/// Verifies break 2 exits both the inner and outer loop from within the innermost loop.
#[test]
fn test_multilevel_break_exits_nested_loops() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    echo "i" . $i . ":";
    for ($j = 0; $j < 3; $j++) {
        if ($i == 1) { break 2; }
        echo $j;
    }
}
echo "end";
"#,
    );
    assert_eq!(out, "i0:012i1:end");
}

/// Verifies continue 2 jumps to the outer loop's update expression, skipping the inner loop body and outer echo.
#[test]
fn test_multilevel_continue_targets_outer_loop_update() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    echo "i" . $i . ":";
    for ($j = 0; $j < 3; $j++) {
        if ($j == 1) { continue 2; }
        echo $j;
    }
    echo "x";
}
echo "end";
"#,
    );
    assert_eq!(out, "i0:0i1:0i2:0end");
}

/// Verifies continue 2 from within a switch targets the outer for loop, not the switch itself.
#[test]
fn test_multilevel_continue_from_switch_targets_outer_loop() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    echo "a";
    switch ($i) {
        case 1:
            echo "b";
            continue 2;
        default:
            echo "c";
    }
    echo "d";
}
"#,
    );
    assert_eq!(out, "acdabacd");
}

/// Verifies break 2 through a try/finally runs the finally block exactly once before exiting.
#[test]
fn test_multilevel_break_through_finally_runs_finally_once() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 2; $i++) {
    for ($j = 0; $j < 2; $j++) {
        try {
            echo "t";
            break 2;
        } finally {
            echo "f";
        }
    }
    echo "x";
}
echo "e";
"#,
    );
    assert_eq!(out, "tfe");
}

// --- for ---

/// Verifies a basic for loop with manual increment in body iterates 5 times and prints 01234.
#[test]
fn test_for_loop() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i = $i + 1) { echo $i; }");
    assert_eq!(out, "01234");
}

/// Verifies break exits the for loop when $i reaches 3, producing 012.
#[test]
fn test_for_break() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 10; $i = $i + 1) { if ($i == 3) { break; } echo $i; }",
    );
    assert_eq!(out, "012");
}

// --- FizzBuzz ---

/// Verifies nested if/elseif/else chain correctly maps 1–15 to Fizz/Buzz/FizzBuzz/decimal output.
#[test]
fn test_fizzbuzz() {
    let source = r#"<?php
$i = 1;
while ($i <= 15) {
    if ($i % 15 == 0) {
        echo "FizzBuzz\n";
    } elseif ($i % 3 == 0) {
        echo "Fizz\n";
    } elseif ($i % 5 == 0) {
        echo "Buzz\n";
    } else {
        echo $i;
        echo "\n";
    }
    $i = $i + 1;
}
"#;
    let out = compile_and_run(source);
    assert_eq!(
        out,
        "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n"
    );
}

// --- Increment/Decrement ---

/// Verifies for loop with ++$i post-increment in update expression prints 01234.
#[test]
fn test_for_with_increment() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i++) { echo $i; }");
    assert_eq!(out, "01234");
}

/// Verifies pre-increment (++$i) updates before the loop body echo, producing 123.
#[test]
fn test_while_with_pre_increment() {
    let out = compile_and_run("<?php $i = 0; while ($i < 3) { ++$i; echo $i; }");
    assert_eq!(out, "123");
}

// --- Functions ---

/// Verifies null is falsy in an if condition, selecting the else branch.
#[test]
fn test_if_null_is_falsy() {
    let out = compile_and_run(
        r#"<?php
$x = null;
if ($x) {
    echo "true";
} else {
    echo "false";
}
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies null as a while condition prevents loop entry and prints ok.
#[test]
fn test_while_null_no_loop() {
    let out = compile_and_run("<?php $x = null; while ($x) { echo \"bad\"; } echo \"ok\";");
    assert_eq!(out, "ok");
}

/// Issue #476: a `for` update clause accepts an array push, and the pushed element is the
/// value the clause sees at the end of each iteration.
///
/// The clause used to go through a bespoke mini-grammar that recognised only `$v++`, `++$v`
/// and `$v = / op= / ??= expr`, so `$a[] = $i` failed to PARSE with
/// `Expected '=' after variable name`. PHP prints `3|2`.
#[test]
fn test_for_update_clause_accepts_an_array_push() {
    let out = compile_and_run(
        "<?php $a = []; $i = 0; for (; $i < 3; $a[] = $i) { $i++; } echo count($a), \"|\", $a[1];",
    );
    assert_eq!(out, "3|2");
}

/// Issue #476: both `for` clauses accept a COMMA-SEPARATED list, which is what PHP's grammar
/// has always been. The update form used to fail with `Expected ')' after for clauses` and
/// the init form — not reported on the issue, same gap — with `Expected ';'`.
#[test]
fn test_for_clauses_accept_comma_separated_lists() {
    let update = compile_and_run(
        "<?php $b = []; for ($i = 0; $i < 3; $i++, $b[] = $i) {} echo count($b), \"|\", $b[0];",
    );
    assert_eq!(update, "3|1");

    let init = compile_and_run(
        "<?php for ($x = 0, $y = 10; $x < 2; $x++) {} echo $x, \"|\", $y;",
    );
    assert_eq!(init, "2|10");
}

/// Issue #476: the same gap rejected an indexed assignment and a property assignment in the
/// update clause. Neither is on the issue; both fell into the same mini-grammar error arms
/// (`Expected '=' after variable name` and `Invalid assignment target`).
///
/// The indexed case also pins that the clause runs AFTER the body: `$i` is already 3 on the
/// final pass, so PHP grows the array to four elements rather than three.
#[test]
fn test_for_update_clause_accepts_indexed_and_property_assignment() {
    let indexed = compile_and_run(
        "<?php $c = [0, 0, 0]; for ($i = 0; $i < 3; $c[$i] = $i * 2) { $i++; } echo implode(\",\", $c);",
    );
    assert_eq!(indexed, "0,2,4,6");

    let property = compile_and_run(
        "<?php class Box { public int $n = 0; } $box = new Box(); \
         for ($i = 0; $i < 4; $box->n = $i) { $i++; } echo $box->n;",
    );
    assert_eq!(property, "4");
}

/// Issue #476 regression guard: a comma nested inside a call's argument list, an array
/// literal or an index is NOT a clause separator, and an empty clause still parses.
///
/// This is the half a naive comma split gets wrong — `for ($i = max(0, 1); …)` would be cut
/// in two at the argument comma.
#[test]
fn test_for_clause_commas_respect_nesting_and_empty_clauses() {
    let call = compile_and_run(
        "<?php for ($i = max(0, 1); $i < 3; $i = min($i + 1, 9)) {} echo $i;",
    );
    assert_eq!(call, "3");

    let literal = compile_and_run(
        "<?php for ($p = [1, 2, 3], $i = 0; $i < 2; $i++) {} echo count($p), \"|\", $i;",
    );
    assert_eq!(literal, "3|2");

    let empty = compile_and_run(
        "<?php $n = 0; for (;;) { $n++; if ($n > 2) { break; } } echo $n;",
    );
    assert_eq!(empty, "3");
}

/// Issue #476, the case that pins the depth tracking on BOTH axes at once: a closure in the
/// init clause puts a `;` inside braces (which must not end the clause) ahead of the comma
/// that really separates it from the next piece (which must).
///
/// A slicer that stopped at the first `;`, or split at the first `,`, would cut this clause
/// in half. PHP prints `10|3`.
#[test]
fn test_for_init_clause_accepts_a_closure_containing_a_semicolon() {
    let out = compile_and_run(
        "<?php for ($f = function (int $n): int { return $n * 2; }, $i = 0; $i < 3; $i++) {} \
         echo $f(5), \"|\", $i;",
    );
    assert_eq!(out, "10|3");
}

/// Issue #476 review follow-up: `throw` is an EXPRESSION in PHP 8, so it belongs in a clause.
///
/// The first version of the clause allow-list listed `throw` among the statements to reject,
/// on the reasoning that it is a statement in PHP's grammar. That was true before PHP 8.0 and
/// is not now: `for (throw new LogicException("init"); false; )` and a `throw` in the update
/// clause both run on 8.5.10, and the output below was measured there.
#[test]
fn test_for_clause_throw_is_an_expression() {
    let out = compile_and_run(
        r#"<?php
try {
    for (throw new LogicException("init"); false; ) {
    }
} catch (LogicException $e) {
    echo "init:", $e->getMessage(), "\n";
}

try {
    for ($i = 0; $i < 3; throw new RuntimeException("update")) {
        echo $i;
    }
} catch (RuntimeException $e) {
    echo "|update:", $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(out, "init:init\n0|update:update\n");
}

// --- Ternary operator ---
