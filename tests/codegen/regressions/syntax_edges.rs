//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions syntax edges, including spread into named params, spread into named params three, and braceless if.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_spread_into_named_params() {
    // Verifies positional spread args (`...$array`) correctly fill a function's
    // positional parameters. Fixture: 2-element array unpacked into 2-param function.
    let out = compile_and_run(
        r#"<?php
function add($a, $b) { return $a + $b; }
$args = [3, 4];
echo add(...$args);
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_spread_into_named_params_three() {
    // Verifies positional spread args with 3 elements unpacked into a 3-param function.
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) { return $a + $b + $c; }
$args = [10, 20, 30];
echo sum3(...$args);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_braceless_if() {
    // Verifies single-statement `if` without braces compiles and executes correctly.
    let out = compile_and_run(
        r#"<?php
if (true) echo "yes";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_multi_argument_echo() {
    // Verifies `echo` with multiple comma-separated arguments outputs each in sequence.
    let out = compile_and_run(
        r#"<?php
echo "A", "B", 3, "\n";
"#,
    );
    assert_eq!(out, "AB3\n");
}

#[test]
fn test_braceless_if_else() {
    // Verifies braceless `if`/`else` with single-statement branches executes the correct branch.
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else echo "small";
"#,
    );
    assert_eq!(out, "small");
}

#[test]
fn test_braceless_for() {
    // Verifies single-statement `for` loop without braces iterates 0..2.
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) echo $i;
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_braceless_while() {
    // Verifies single-statement `while` loop without braces iterates post-increment 0..2.
    let out = compile_and_run(
        r#"<?php
$i = 0;
while ($i < 3) echo $i++;
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_braceless_foreach() {
    // Verifies single-statement `foreach` without braces iterates array values 1..3.
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
foreach ($arr as $v) echo $v;
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_braceless_else_if() {
    // Verifies braceless `if` / `else if` / `else` chain with single-statement branches
    // selects the correct branch based on condition $x > 10 > 3.
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else if ($x > 3) echo "medium";
else echo "small";
"#,
    );
    assert_eq!(out, "medium");
}

// --- Bug regression tests ---
