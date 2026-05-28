//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, named arguments spread, including named arguments after spread for user function, named arguments after spread uses default for unpacked gap, and spread only uses default for unpacked optional param.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

/// Verifies named arguments after a positional spread for a user function;
/// `sum3(...$args, c: 30)` with `$args = [10, 20]` outputs "60".
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

/// Verifies named arguments after a short spread fill gaps using defaults;
/// `sum3(...$args, c: 30)` with `$args = [10]` uses default `$b = 2` and `$c = 30`, output "42".
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

/// Verifies a spread-only call fills optional params with defaults; `show(...[10])` outputs "10:99".
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

/// Verifies a spread-only call with a positional prefix uses defaults for the optional tail;
/// `show(10, ...[])` outputs "10:99".
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

/// Verifies that a spread-only call with too few elements for a required parameter produces a fatal error
/// "Fatal error: too few arguments for spread call".
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

/// Verifies an associative spread literal with string keys maps them to named arguments;
/// `show(...["a" => 10])` outputs "10:99" (a=10, b uses default).
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

/// Verifies an associative spread literal preserves the order of string keys for named argument matching;
/// `show(...["b" => 20, "a" => 10])` outputs "10:20".
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

/// Verifies an associative spread literal with mixed numeric and string keys treats string keys as
/// named and numeric keys as positional; `show(...[0 => 10, "b" => 20])` outputs "10:20".
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

/// Verifies that in an assoc spread, string keys come first and numeric keys are appended after them;
/// `show(...["a" => 1, 1 => 2])` outputs "1:2".
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

/// Verifies that duplicate string keys in an assoc spread literal use the last value (PHP behavior);
/// `show(...["a" => 1, "a" => 2])` outputs "2".
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

/// Verifies an assoc spread literal works for builtin calls;
/// `str_repeat(...["string" => "ha", "times" => 3])` outputs "hahaha".
#[test]
fn test_assoc_spread_literal_for_builtin_call() {
    let out = compile_and_run(
        r#"<?php
echo str_repeat(...["string" => "ha", "times" => 3]);
"#,
    );
    assert_eq!(out, "hahaha");
}

/// Verifies an assoc spread variable can supply named args and be combined with an explicit named arg;
/// `show(...$args, c: 3)` with `$args = ["b" => 2, "a" => 1]` outputs "1:2:3".
#[test]
fn test_assoc_spread_variable_maps_string_keys_to_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b, $c = 0) {
    echo $a . ":" . $b . ":" . $c;
}
$args = ["b" => 2, "a" => 1];
show(...$args, c: 3);
"#,
    );
    assert_eq!(out, "1:2:3");
}

/// Verifies an assoc spread variable without explicit named args supplies all params;
/// `show(...$args)` with `$args = ["b" => 20, "a" => 10]` outputs "10:20".
#[test]
fn test_assoc_spread_variable_without_explicit_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b) {
    echo $a . ":" . $b;
}
$args = ["b" => 20, "a" => 10];
show(...$args);
"#,
    );
    assert_eq!(out, "10:20");
}

/// Verifies an assoc spread variable supplies params before explicit named args;
/// `sum3(...$args, a: 10, b: 20)` with `$args = ["c" => 30]` outputs "60".
#[test]
fn test_assoc_spread_variable_supplies_param_after_explicit_named_args() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = ["c" => 30];
echo sum3(...$args, a: 10, b: 20);
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies an assoc spread variable supplies only the params it specifies, with defaults filling the rest;
/// `show(...$args, a: 10)` with `$args = ["d" => 400]` outputs "10:20:30:400".
#[test]
fn test_assoc_spread_variable_uses_defaults_for_skipped_params() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b = 20, $c = 30, $d = 40) {
    echo $a . ":" . $b . ":" . $c . ":" . $d;
}
$args = ["d" => 400];
show(...$args, a: 10);
"#,
    );
    assert_eq!(out, "10:20:30:400");
}

/// Verifies a positional spread followed by an assoc spread variable followed by named args fills all params;
/// `sum3(...$pos, ...$args, b: 20)` with `$pos = [10]` and `$args = ["c" => 30]` outputs "60".
#[test]
fn test_assoc_spread_variable_after_positional_spread_supplies_named_gap() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$pos = [10];
$args = ["c" => 30];
echo sum3(...$pos, ...$args, b: 20);
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies an assoc spread variable can supply a builtin parameter after an explicit named arg;
/// `str_repeat(...$args, string: "ha")` with `$args = ["times" => 3]` outputs "hahaha".
#[test]
fn test_assoc_spread_variable_supplies_builtin_param_after_explicit_named_arg() {
    let out = compile_and_run(
        r#"<?php
$args = ["times" => 3];
echo str_repeat(...$args, string: "ha");
"#,
    );
    assert_eq!(out, "hahaha");
}

/// Verifies an assoc spread variable supplies closure params after explicit named args;
/// `$sum3(...$args, a: 10, b: 20)` with `$args = ["c" => 30]` outputs "60".
#[test]
fn test_assoc_spread_variable_supplies_closure_param_after_explicit_named_args() {
    let out = compile_and_run(
        r#"<?php
$sum3 = function ($a, $b, $c) {
    return $a + $b + $c;
};
$args = ["c" => 30];
echo $sum3(...$args, a: 10, b: 20);
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies an assoc spread variable supplies first-class callable params after explicit named args;
/// `$sum(...$args, a: 10, b: 20)` with `$args = ["c" => 30]` outputs "60".
#[test]
fn test_assoc_spread_variable_supplies_first_class_callable_param_after_explicit_named_args() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$sum = sum3(...);
$args = ["c" => 30];
echo $sum(...$args, a: 10, b: 20);
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies an assoc spread variable supplies constructor params after explicit named args;
/// `new Total(...$args, a: 10, b: 20)` with `$args = ["c" => 30]` outputs "60".
#[test]
fn test_assoc_spread_variable_supplies_constructor_param_after_explicit_named_args() {
    let out = compile_and_run(
        r#"<?php
class Total {
    public $value;

    public function __construct($a, $b, $c) {
        $this->value = $a + $b + $c;
    }
}

$args = ["c" => 30];
$total = new Total(...$args, a: 10, b: 20);
echo $total->value;
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies that a spread with too few elements followed by a named argument produces a fatal error
/// "Fatal error: named argument spread length mismatch".
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

/// Verifies that a spread with too many elements followed by a named argument produces a fatal error
/// "Fatal error: named argument spread length mismatch".
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

/// Verifies source evaluation order is preserved for named arguments;
/// `sum2(b: mark("b", 2), a: mark("a", 1))` outputs "ba:3" (b marker fires before a).
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

/// Verifies a spread is evaluated exactly once when followed by a named argument;
/// `sum3(...args(), c: last())` outputs "xc:60" (x from args, c from last).
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

/// Verifies multiple spreads followed by a named argument are each evaluated once and fill positional params;
/// `sum3(...first(), ...second(), c: last())` outputs "abc:6".
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

/// Verifies that when a spread with excess elements is followed by a named argument, the runtime error
/// fires after the spread and named args are evaluated; stdout is "sc" and stderr contains the mismatch error.
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
