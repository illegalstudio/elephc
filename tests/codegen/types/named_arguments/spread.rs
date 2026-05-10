//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, named arguments spread, including named arguments after spread for user function, named arguments after spread uses default for unpacked gap, and spread only uses default for unpacked optional param.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

#[test]
fn test_named_arguments_after_spread_for_user_function() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = [10, 20];
echo sum3(...$args, c: 30);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_named_arguments_after_spread_uses_default_for_unpacked_gap() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b = 2, $c = 3) {
    return $a + $b + $c;
}
$args = [10];
echo sum3(...$args, c: 30);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_spread_only_uses_default_for_unpacked_optional_param() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b = 99) {
    echo $a . ":" . $b;
}
show(...[10]);
"#,
    );
    assert_eq!(out, "10:99");
}

#[test]
fn test_spread_only_positional_prefix_uses_default_for_optional_tail() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b = 99) {
    echo $a . ":" . $b;
}
show(10, ...[]);
"#,
    );
    assert_eq!(out, "10:99");
}

#[test]
fn test_spread_only_rejects_missing_required_param() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function show($a, $b) {
    echo $a . ":" . $b;
}
show(...[10]);
"#,
    );
    assert!(err.contains("Fatal error: too few arguments for spread call"));
}

#[test]
fn test_assoc_spread_literal_maps_string_keys_to_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b = 99) {
    echo $a . ":" . $b;
}
show(...["a" => 10]);
"#,
    );
    assert_eq!(out, "10:99");
}

#[test]
fn test_assoc_spread_literal_preserves_key_order_for_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b) {
    echo $a . ":" . $b;
}
show(...["b" => 20, "a" => 10]);
"#,
    );
    assert_eq!(out, "10:20");
}

#[test]
fn test_assoc_spread_literal_mixes_numeric_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b) {
    echo $a . ":" . $b;
}
show(...[0 => 10, "b" => 20]);
"#,
    );
    assert_eq!(out, "10:20");
}

#[test]
fn test_assoc_spread_literal_reorders_numeric_after_string_key() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b) {
    echo $a . ":" . $b;
}
show(...["a" => 1, 1 => 2]);
"#,
    );
    assert_eq!(out, "1:2");
}

#[test]
fn test_assoc_spread_literal_duplicate_string_key_uses_last_value() {
    let out = compile_and_run(
        r#"<?php
function show($a) {
    echo $a;
}
show(...["a" => 1, "a" => 2]);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_assoc_spread_literal_for_builtin_call() {
    let out = compile_and_run(
        r#"<?php
echo str_repeat(...["string" => "ha", "times" => 3]);
"#,
    );
    assert_eq!(out, "hahaha");
}

#[test]
fn test_named_arguments_after_spread_rejects_short_spread() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = [10];
echo sum3(...$args, c: 30);
"#,
    );
    assert!(err.contains("Fatal error: named argument spread length mismatch"));
}

#[test]
fn test_named_arguments_after_spread_rejects_overwrite() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = [10, 20, 99];
echo sum3(...$args, c: 30);
"#,
    );
    assert!(err.contains("Fatal error: named argument spread length mismatch"));
}

#[test]
fn test_named_arguments_preserve_source_evaluation_order() {
    let out = compile_and_run(
        r#"<?php
function mark($label, $value) {
    echo $label;
    return $value;
}
function sum2($a, $b) {
    echo ":";
    echo $a + $b;
}
sum2(b: mark("b", 2), a: mark("a", 1));
"#,
    );
    assert_eq!(out, "ba:3");
}

#[test]
fn test_named_arguments_after_spread_evaluate_spread_once() {
    let out = compile_and_run(
        r#"<?php
function args() {
    echo "x";
    return [10, 20];
}
function last() {
    echo "c";
    return 30;
}
function sum3($a, $b, $c) {
    echo ":";
    echo $a + $b + $c;
}
sum3(...args(), c: last());
"#,
    );
    assert_eq!(out, "xc:60");
}

#[test]
fn test_named_arguments_after_multiple_spreads() {
    let out = compile_and_run(
        r#"<?php
function first() {
    echo "a";
    return [1];
}
function second() {
    echo "b";
    return [2];
}
function last() {
    echo "c";
    return 3;
}
function sum3($a, $b, $c) {
    echo ":";
    echo $a + $b + $c;
}
sum3(...first(), ...second(), c: last());
"#,
    );
    assert_eq!(out, "abc:6");
}

#[test]
fn test_named_arguments_after_spread_evaluate_later_named_before_runtime_error() {
    let out = compile_and_run_capture(
        r#"<?php
function args() {
    echo "s";
    return [1, 2, 99];
}
function last() {
    echo "c";
    return 30;
}
function sum3($a, $b, $c) {
    echo $a + $b + $c;
}
sum3(...args(), c: last());
"#,
    );
    assert!(!out.success);
    assert_eq!(out.stdout, "sc");
    assert!(out.stderr.contains("Fatal error: named argument spread length mismatch"));
}
