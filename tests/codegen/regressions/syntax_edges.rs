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
    let out = compile_and_run(
        r#"<?php
if (true) echo "yes";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_braceless_if_else() {
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
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) echo $i;
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_braceless_while() {
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
