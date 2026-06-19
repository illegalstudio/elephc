//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions syntax edges, including spread into named params, spread into named params three, and braceless if.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies positional spread args (`...$array`) correctly fill a function's
/// positional parameters. Fixture: 2-element array unpacked into 2-param function.
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

/// Verifies positional spread args with 3 elements unpacked into a 3-param function.
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

/// Verifies single-statement `if` without braces compiles and executes correctly.
#[test]
fn test_braceless_if() {
    let out = compile_and_run(
        r#"<?php
if (true) echo "yes";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies `echo` with multiple comma-separated arguments outputs each in sequence.
#[test]
fn test_multi_argument_echo() {
    let out = compile_and_run(
        r#"<?php
echo "A", "B", 3, "\n";
"#,
    );
    assert_eq!(out, "AB3\n");
}

/// Verifies braceless `if`/`else` with single-statement branches executes the correct branch.
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

/// Verifies single-statement `for` loop without braces iterates 0..2.
#[test]
fn test_braceless_for() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) echo $i;
"#,
    );
    assert_eq!(out, "012");
}

/// Verifies single-statement `while` loop without braces iterates post-increment 0..2.
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

/// Verifies single-statement `foreach` without braces iterates array values 1..3.
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

/// Verifies braceless `if` / `else if` / `else` chain with single-statement branches
/// selects the correct branch based on condition $x > 10 > 3.
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

/// Regression: a class whose only instantiation appears inside a multi-value `echo` must still
/// be collected for method emission. PHP's multi-argument `echo a, b;` lowers to a `Synthetic`
/// statement sequence; `collect_required_class_names` previously skipped `Synthetic` bodies, so
/// the class (here `SplObjectStorage`, an injected builtin emitted on demand) had its
/// `__construct` referenced by the `new` call site but never emitted → undefined-symbol link
/// failure. A user-declared class is unaffected because its `ClassDecl` always seeds the set;
/// only on-demand/injected classes exposed the gap.
#[test]
fn test_class_used_only_in_multi_value_echo_is_emitted() {
    let out = compile_and_run(r#"<?php echo "x", (new SplObjectStorage())->count();"#);
    assert_eq!(out, "x0");
}
