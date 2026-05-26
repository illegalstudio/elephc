//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow branches and loops, including if true, if false, and if else.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_if_true() {
    // Verifies that if (true) executes the branch.
    let out = compile_and_run("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(out, "yes");
}

#[test]
fn test_if_false() {
    // Verifies that if (false) skips the branch, producing no output.
    let out = compile_and_run("<?php if (1 == 2) { echo \"yes\"; }");
    assert_eq!(out, "");
}

#[test]
fn test_if_else() {
    // Verifies if/else selects the else branch when the condition is false.
    let out = compile_and_run("<?php if (1 == 2) { echo \"a\"; } else { echo \"b\"; }");
    assert_eq!(out, "b");
}

#[test]
fn test_if_elseif_else() {
    // Verifies elseif chain takes the second branch when first condition is false.
    let out = compile_and_run(
        "<?php $x = 2; if ($x == 1) { echo \"one\"; } elseif ($x == 2) { echo \"two\"; } else { echo \"other\"; }",
    );
    assert_eq!(out, "two");
}

#[test]
fn test_if_else_falls_through() {
    // Verifies elseif/else falls through to the else branch when all conditions are false.
    let out = compile_and_run(
        "<?php $x = 99; if ($x == 1) { echo \"a\"; } elseif ($x == 2) { echo \"b\"; } else { echo \"c\"; }",
    );
    assert_eq!(out, "c");
}

// --- while ---

#[test]
fn test_while_loop() {
    // Verifies a basic while loop iterates 5 times and prints 01234.
    let out = compile_and_run("<?php $i = 0; while ($i < 5) { echo $i; $i = $i + 1; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_while_zero_iterations() {
    // Verifies while (false) body never executes.
    let out = compile_and_run("<?php while (0) { echo \"no\"; }");
    assert_eq!(out, "");
}

#[test]
fn test_while_break() {
    // Verifies break exits the while loop when $i reaches 3, producing 012.
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 10) { if ($i == 3) { break; } echo $i; $i = $i + 1; }",
    );
    assert_eq!(out, "012");
}

#[test]
fn test_while_continue() {
    // Verifies continue skips to the next iteration, skipping the echo at $i==3.
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 5) { $i = $i + 1; if ($i == 3) { continue; } echo $i; }",
    );
    assert_eq!(out, "1245");
}

#[test]
fn test_multilevel_break_exits_nested_loops() {
    // Verifies break 2 exits both the inner and outer loop from within the innermost loop.
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

#[test]
fn test_multilevel_continue_targets_outer_loop_update() {
    // Verifies continue 2 jumps to the outer loop's update expression, skipping the inner loop body and outer echo.
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

#[test]
fn test_multilevel_continue_from_switch_targets_outer_loop() {
    // Verifies continue 2 from within a switch targets the outer for loop, not the switch itself.
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

#[test]
fn test_multilevel_break_through_finally_runs_finally_once() {
    // Verifies break 2 through a try/finally runs the finally block exactly once before exiting.
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

#[test]
fn test_for_loop() {
    // Verifies a basic for loop with manual increment in body iterates 5 times and prints 01234.
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i = $i + 1) { echo $i; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_for_break() {
    // Verifies break exits the for loop when $i reaches 3, producing 012.
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 10; $i = $i + 1) { if ($i == 3) { break; } echo $i; }",
    );
    assert_eq!(out, "012");
}

// --- FizzBuzz ---

#[test]
fn test_fizzbuzz() {
    // Verifies nested if/elseif/else chain correctly maps 1–15 to Fizz/Buzz/FizzBuzz/decimal output.
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

#[test]
fn test_for_with_increment() {
    // Verifies for loop with ++$i post-increment in update expression prints 01234.
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i++) { echo $i; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_while_with_pre_increment() {
    // Verifies pre-increment (++$i) updates before the loop body echo, producing 123.
    let out = compile_and_run("<?php $i = 0; while ($i < 3) { ++$i; echo $i; }");
    assert_eq!(out, "123");
}

// --- Functions ---

#[test]
fn test_if_null_is_falsy() {
    // Verifies null is falsy in an if condition, selecting the else branch.
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

#[test]
fn test_while_null_no_loop() {
    // Verifies null as a while condition prevents loop entry and prints ok.
    let out = compile_and_run("<?php $x = null; while ($x) { echo \"bad\"; } echo \"ok\";");
    assert_eq!(out, "ok");
}

// --- Ternary operator ---
