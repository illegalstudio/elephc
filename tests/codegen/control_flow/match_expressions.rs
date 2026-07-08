//! Purpose:
//! Integration and regression tests for match expressions with heterogeneous
//! arm result types (object/array/string/int/null mixes) and homogeneous arms.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Regression coverage for issue #488: heterogeneous match arms must merge
//!   to a Mixed hidden temp (boxed per arm) instead of coercing every arm to
//!   one scalar-biased unified type, which fatally cast object arms to string.
//! - `gettype` probes assert PHP's per-arm type preservation across arms.

use super::*;

/// Regression test for issue #488: a match whose arms produce an object, an
/// array, and a string must preserve each arm's value instead of fatally
/// coercing the object arm to string.
#[test]
fn test_match_heterogeneous_object_array_string_arms() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n % 3) {
        0 => new stdClass(),
        1 => [1, 2],
        default => "s",
    };
}
$count = 0;
for ($i = 0; $i < 40; $i++) {
    $v = pick($i);
    $count++;
}
echo $count;
"#,
    );
    assert_eq!(out, "40");
}

/// Tests that object and string match arms each keep their own runtime type
/// (PHP prints "object|string"), in both arm orders.
#[test]
fn test_match_object_and_string_arms_preserve_types() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => new stdClass(),
        default => "s",
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "object|string");
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => "s",
        default => new stdClass(),
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "string|object");
}

/// Tests that object and array match arms are not silently unified: PHP
/// reports "object|array" for the two reached arms.
#[test]
fn test_match_object_and_array_arms_preserve_types() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => new stdClass(),
        default => [1, 2],
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "object|array");
}

/// Tests that an object arm mixed with an int arm compiles and preserves both
/// runtime types (previously failed with an unsupported object-to-int cast).
#[test]
fn test_match_object_and_int_arms_preserve_types() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => new stdClass(),
        default => 7,
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "object|integer");
}

/// Tests that array and int match arms keep their own types instead of the
/// int arm silently absorbing the array arm (or vice versa by arm order).
#[test]
fn test_match_array_and_int_arms_preserve_types() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => [1, 2],
        default => 7,
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "array|integer");
}

/// Tests that string and int match arms keep their own types instead of the
/// string arm absorbing the int arm into a unified string temp.
#[test]
fn test_match_string_and_int_arms_preserve_types() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => "s",
        default => 7,
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "string|integer");
}

/// Tests that object and null match arms preserve both results: PHP reports
/// "object|NULL" instead of coercing the null arm into an object.
#[test]
fn test_match_object_and_null_arms_preserve_types() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => new stdClass(),
        default => null,
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "object|NULL");
}

/// Tests a heterogeneous match consumed at the top level (no enclosing
/// function or declared return type): the fatal object-to-string cast
/// happened inside the match arm itself, before any return coercion.
#[test]
fn test_match_heterogeneous_arms_top_level() {
    let out = compile_and_run(
        r#"<?php
$n = 0;
$v = match($n) {
    0 => new stdClass(),
    default => "s",
};
echo gettype($v);
"#,
    );
    assert_eq!(out, "object");
}

/// Tests a heterogeneous match returned from a function with no declared
/// return type: the checker's inferred match type must agree with the
/// lowered Mixed merge instead of erroring on an unsupported cast.
#[test]
fn test_match_heterogeneous_arms_inferred_return_type() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n) {
    return match($n) {
        0 => new stdClass(),
        default => "s",
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "object|string");
}

/// Tests that homogeneous string arms keep working (the merged temp stays a
/// plain string, no boxing regression for same-typed arms).
#[test]
fn test_match_homogeneous_string_arms() {
    let out = compile_and_run(
        r#"<?php
$n = 1;
echo match($n) {
    0 => "zero",
    1 => "one",
    default => "many",
};
"#,
    );
    assert_eq!(out, "one");
}

/// Tests that a throw-only default arm does not widen the merged match type:
/// the value arms stay strings and the non-throw path prints normally.
#[test]
fn test_match_throw_default_arm_keeps_value_arm_type() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): string {
    return match($n) {
        0 => "zero",
        1 => "one",
        default => throw new Exception("nope"),
    };
}
echo pick(0), "|", pick(1);
"#,
    );
    assert_eq!(out, "zero|one");
}

/// Regression test for the gettype() dispatch on a match-produced nullable
/// int: the hidden temp is an inline tagged scalar (`null|int`), which the
/// gettype() emitter previously unboxed as a boxed Mixed cell and crashed.
#[test]
fn test_match_int_and_null_arms_gettype() {
    let out = compile_and_run(
        r#"<?php
$v = match($argc) {
    1 => 1,
    default => null,
};
echo gettype($v), "|";
$w = match($argc) {
    99 => 1,
    default => null,
};
echo gettype($w);
"#,
    );
    assert_eq!(out, "integer|NULL");
}

/// Tests that a null arm survives a return with an inferred type: the checker
/// must infer a nullable merge (like the lowered temp) instead of dropping the
/// null arm and coercing its value to the other arm's type.
#[test]
fn test_match_null_and_string_arms_inferred_return_keeps_null() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n) {
    return match($n) {
        0 => null,
        default => "s",
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "NULL|string");
}

/// Tests the mirror arm order with an int value arm: the inferred return type
/// must stay nullable so the null default is not coerced to int 0.
#[test]
fn test_match_int_and_null_default_inferred_return_keeps_null() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n) {
    return match($n) {
        0 => 7,
        default => null,
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "integer|NULL");
}

/// Tests that heterogeneous arm values survive being consumed (not just type
/// probed): a corrupted boxed payload with a correct tag would pass the
/// gettype tests but fail here.
#[test]
fn test_match_heterogeneous_arm_values_consumed() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => "s",
        default => 7,
    };
}
echo pick(0), "|", pick(1);
"#,
    );
    assert_eq!(out, "s|7");
}

/// Tests that float and int match arms keep their own types and values: the
/// (Int, Float) pair must widen to Mixed, not unify to float.
#[test]
fn test_match_float_and_int_arms_preserve_types_and_values() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n) {
    return match($n) {
        0 => 6.5,
        default => 8,
    };
}
echo gettype(pick(0)), "|", gettype(pick(1)), "|", pick(0), "|", pick(1);
"#,
    );
    assert_eq!(out, "double|integer|6.5|8");
}

/// Tests a heterogeneous match nested as another match's arm result: the
/// inner match must contribute its merged (Mixed) type to the outer merge
/// instead of a scalar-biased syntactic fallback re-introducing the #488
/// fatal cast for nested arms.
#[test]
fn test_match_nested_heterogeneous_match_arm() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): mixed {
    return match($n) {
        0 => match(0) {
            0 => new stdClass(),
            default => 1,
        },
        default => "s",
    };
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "object|string");
}

/// Pins the documented int/bool arm-merge incompatibility (see "Known
/// incompatibilities with PHP" in docs/php/types.md): int and bool arms share
/// one runtime representation, so both arms observe it — PHP would print
/// "integer|boolean" here. If this test starts failing with PHP's output, the
/// incompatibility got fixed: update the docs entry alongside.
#[test]
fn test_match_int_and_bool_arms_merge_documented_divergence() {
    let out = compile_and_run(
        r#"<?php
$r = match($argc) {
    1 => 42,
    default => true,
};
$w = match($argc) {
    99 => 42,
    default => true,
};
echo gettype($r), "|", gettype($w);
"#,
    );
    assert_eq!(out, "boolean|boolean");
}

/// Tests that heterogeneous scalar match arms stay heap-balanced: the boxed
/// per-arm values and the hidden Mixed temp must release cleanly.
#[test]
fn test_match_heterogeneous_scalar_arms_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function pick(int $n): mixed {
    return match($n % 2) {
        0 => "s",
        default => 7,
    };
}
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $v = pick($i);
    $count++;
}
echo $count;
"#,
    );
    assert_eq!(out.stdout, "20");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap summary, got: {}",
        out.stderr
    );
}
