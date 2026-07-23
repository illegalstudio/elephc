//! Purpose:
//! Integration and regression tests for match expressions, including builtin
//! error references and heterogeneous or homogeneous arm result types.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Regression coverage for issue #488: heterogeneous match arms must merge
//!   to a Mixed hidden temp (boxed per arm) instead of coercing every arm to
//!   one scalar-biased unified type, which fatally cast object arms to string.
//! - Regression coverage for issue #549: match arms producing indexed or
//!   associative arrays with mismatched element types must widen the merged
//!   temp to array-of-boxed-Mixed elements instead of letting the last arm's
//!   element type relabel every other arm's runtime array.
//! - `gettype` probes assert PHP's per-arm type preservation across arms.
//! - Explicit `UnhandledMatchError` construction is distinct from the current
//!   fatal terminator used when no match arm and no default arm succeeds.

use super::*;

/// Verifies the builtin `UnhandledMatchError` class can be constructed in a
/// match default arm, thrown, caught by its fully-qualified name, and queried
/// through the `getMessage()` method inherited from `Error`.
#[test]
fn test_unhandled_match_error_throw_and_catch() {
    let out = compile_and_run(
        r#"<?php
function classify(int $n): string {
    return match (true) {
        $n < 0 => "negative",
        $n === 0 => "zero",
        default => throw new UnhandledMatchError("no arm for " . $n),
    };
}

try {
    classify(5);
} catch (\UnhandledMatchError $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "no arm for 5");
}

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

/// Regression for the assignment-effects Match path: assigning a heterogeneous
/// match to a local and returning that local through an inferred return type
/// must keep per-arm types (`object|string`), not reintroduce the Str-absorbing
/// syntactic join that coerced both arms to string.
#[test]
fn test_match_heterogeneous_assign_inferred_return_preserves_types() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n) {
    $v = match($n) {
        0 => new stdClass(),
        default => "s",
    };
    return $v;
}
echo gettype(pick(0)), "|", gettype(pick(1));
"#,
    );
    assert_eq!(out, "object|string");
}

/// Regression for issue #494: object/null match arms must keep the inferred
/// return nullable both directly and after assignment to a local.
#[test]
fn test_match_object_null_inferred_returns_keep_null() {
    let out = compile_and_run(
        r#"<?php
function direct(int $n) {
    return match($n) {
        0 => new stdClass(),
        default => null,
    };
}
function assigned(int $n) {
    $value = match($n) {
        0 => new stdClass(),
        default => null,
    };
    return $value;
}
echo gettype(direct(0)), "|", gettype(direct(1)), "|";
echo gettype(assigned(0)), "|", gettype(assigned(1));
"#,
    );
    assert_eq!(out, "object|NULL|object|NULL");
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

/// Regression test for issue #549: match arms producing indexed arrays with
/// different element types (array<int> vs array<string>) must widen the merged
/// result to array-of-Mixed instead of letting the last arm's element type
/// win. `$argc` is 1 under the test runner, so the int arm is selected; before
/// the fix the merged temp was typed array<string> and reading the int arm's
/// 8-byte scalar slots as 16-byte string descriptors segfaulted (the issue's
/// exact repro).
#[test]
fn test_match_array_int_and_array_string_arms_selects_int_arm() {
    let out = compile_and_run(
        r#"<?php
$r = match($argc) {
    1 => [1, 2],
    default => ["a", "b"],
};
echo $r[0], "\n";
echo $r[1], "\n";
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// Reverse arm order for issue #549: the string arm is selected at runtime
/// while the int arm folds last into the merge; before the fix the temp kept
/// array<int> and the string arm's 16-byte slots were read back as raw heap
/// pointers (silent garbage instead of "a"/"b").
#[test]
fn test_match_array_string_and_array_int_arms_selects_string_arm() {
    let out = compile_and_run(
        r#"<?php
$r = match($argc) {
    1 => ["a", "b"],
    default => [1, 2],
};
echo $r[0], "\n", $r[1], "\n", gettype($r[0]), "\n";
"#,
    );
    assert_eq!(out, "a\nb\nstring\n");
}

/// Associative variant of issue #549: assoc arms with mismatched value types
/// must widen the merged value dimension to boxed Mixed. Before the fix the
/// temp kept the default arm's string value type and the int-valued entries
/// read back as empty strings.
#[test]
fn test_match_assoc_arms_with_mismatched_value_types() {
    let out = compile_and_run(
        r#"<?php
$r = match($argc) {
    1 => ["k" => 1, "n" => 2],
    default => ["k" => "x", "n" => "y"],
};
echo $r["k"], "\n", $r["n"], "\n";
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// Empty-arm variant of issue #549: an `[]` arm (array<never>) must not win
/// the merge wholesale. Before the fix the temp was typed array<never> and
/// every element read of the populated arm returned the null sentinel,
/// printing blank lines instead of the ints.
#[test]
fn test_match_array_arm_with_empty_array_arm() {
    let out = compile_and_run(
        r#"<?php
$r = match($argc) {
    1 => [1, 2],
    default => [],
};
echo $r[0], "\n", $r[1], "\n";
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// Borrowed-source variant of issue #549: when an arm forwards a live local
/// array, widening must copy-on-write instead of boxing the local's slots in
/// place, so the source variable keeps its typed layout after the match.
#[test]
fn test_match_variable_array_arm_preserves_source_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$r = match($argc) {
    1 => $a,
    default => ["x", "y"],
};
echo $r[0], "\n", $r[1], "\n";
echo $a[0], "\n", $a[1], "\n";
"#,
    );
    assert_eq!(out, "1\n2\n1\n2\n");
}

/// Nested-array variant of issue #549: arms of array<array<int>> vs
/// array<array<string>> widen the outer element to Mixed; inner reads must
/// unbox the nested array pointer and still address 8-byte int slots.
#[test]
fn test_match_nested_array_arms_widen_inner_elements() {
    let out = compile_and_run(
        r#"<?php
$r = match($argc) {
    1 => [[1, 2]],
    default => [["a", "b"]],
};
echo $r[0][0], "\n", $r[0][1], "\n";
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// Heap balance for issue #549: the per-arm array-to-Mixed widening transfers
/// slot ownership into the boxed cells, so repeated match merges and element
/// reads must release cleanly (no leak from the conversion or the hidden
/// temp protocol).
#[test]
fn test_match_mismatched_array_arms_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $r = match($i % 2) {
        0 => [1, 2],
        default => ["a", "b"],
    };
    $count = $count + count($r);
    $probe = $r[0];
}
echo $count;
"#,
    );
    assert_eq!(out.stdout, "40");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap summary, got: {}",
        out.stderr
    );
}

/// Sentinel-arm guard for issue #549: a missed indexed read forwarded by a
/// match arm materializes the in-band null-container sentinel; the merge's
/// array-to-Mixed widening must pass it through instead of dereferencing it
/// as an array header (this segfaulted while the fix was unguarded).
#[test]
fn test_match_missed_indexed_read_arm_passes_sentinel_through() {
    let out = compile_and_run(
        r#"<?php
$rows = [[1, 2]];
$r = match($argc) {
    1 => $rows[5],
    default => ["a", "b"],
};
echo $r[0] ?? "none", "\n";
echo "done", "\n";
"#,
    );
    assert_eq!(out, "none\ndone\n");
}

/// Associative sentinel-arm guard for issue #549: a missed hash read
/// forwarded by a match arm must pass the null-container sentinel through the
/// hash-to-Mixed widening instead of iterating its entries.
#[test]
fn test_match_missed_hash_read_arm_passes_sentinel_through() {
    let out = compile_and_run(
        r#"<?php
$maps = [["k" => 1]];
$r = match($argc) {
    1 => $maps[5],
    default => ["k" => "x"],
};
echo $r["k"] ?? "none", "\n";
echo "done", "\n";
"#,
    );
    assert_eq!(out, "none\ndone\n");
}

/// Heap balance for the borrowed-source widening of issue #549: a live local
/// forwarded by an arm reports as a *provisional* owner during lowering, so
/// the conversion must retain it for real before the copy-on-write split;
/// getting that wrong either boxed the local in place or double-released the
/// shared array (heap debug flagged a bad refcount at exit).
#[test]
fn test_match_borrowed_array_arm_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2];
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $r = match($i % 2) {
        0 => $a,
        default => ["a", "b"],
    };
    $count = $count + count($r);
    $probe = $r[0];
}
echo $count, "|", $a[0], "|", $a[1];
"#,
    );
    assert_eq!(out.stdout, "40|1|2");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap summary, got: {}",
        out.stderr
    );
}
