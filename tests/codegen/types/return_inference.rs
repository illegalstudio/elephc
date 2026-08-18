//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types return type inference, including return type from foreach, return type mixed branches, and return type switch foreach.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies return type inference when a `foreach` carries a typed return out of a loop.
/// Fixture: `find()` uses `foreach` with an early `return "found"` and a fallback `return "not found"`.
/// Asserts that the returned string is correct when the target is found.
#[test]
fn test_return_type_from_foreach() {
    let out = compile_and_run(
        r#"<?php
function find($arr, $target) {
    foreach ($arr as $v) {
        if ($v === $target) { return "found"; }
    }
    return "not found";
}
echo find([1, 2, 3], 2);
"#,
    );
    assert_eq!(out, "found");
}

/// Verifies return type inference when branches return different types (`string` vs `int`).
/// The `describe()` function returns `"positive"` in the positive branch and `0` in the else branch.
/// Asserts that the branch that fires produces the correct output.
#[test]
fn test_return_type_mixed_branches() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 0) { return "positive"; }
    return 0;
}
$r = describe(5);
echo $r;
"#,
    );
    assert_eq!(out, "positive");
}

/// Verifies return type inference when a `foreach` with a `switch` carries a typed return.
/// The `classify()` function returns `"zero"` or `"nonzero"` from inside a `switch` inside a `foreach`.
/// Asserts that the correct label is produced.
#[test]
fn test_return_type_switch_foreach() {
    let out = compile_and_run(
        r#"<?php
function classify($items) {
    foreach ($items as $item) {
        switch ($item) {
            case 0: return "zero";
            default: return "nonzero";
        }
    }
    return "empty";
}
echo classify([0]);
"#,
    );
    assert_eq!(out, "zero");
}

/// Verifies return type inference across an `if`/`else` with `string` returns in both branches.
/// The `check()` function returns `"big"` or `"small"` based on `$x > 10`.
/// Asserts both branches produce the correct concatenated output.
#[test]
fn test_return_string_from_else() {
    let out = compile_and_run(
        r#"<?php
function check($x) {
    if ($x > 10) {
        return "big";
    } else {
        return "small";
    }
}
echo check(5) . "|" . check(15);
"#,
    );
    assert_eq!(out, "small|big");
}

/// Verifies that a function with an `array` return type produces an array that is indexable.
/// The `getColor()` function returns `[255, 128, 0]` and the result is indexed with `$color[0]`.
/// Asserts that each array element is accessible and produces the correct values.
#[test]
fn test_array_return_type_survives_indexing() {
    let out = compile_and_run(
        r#"<?php
function getColor(): array {
    return [255, 128, 0];
}

$color = getColor();
echo $color[0] . "," . $color[1] . "," . $color[2];
"#,
    );
    assert_eq!(out, "255,128,0");
}

/// Verifies that `string` elements returned from a typed `array` parameter retain their `string` type
/// when passed to a function expecting `string`. The `pickSecond()` function takes an `array` and
/// passes `$names[1]` to `paint()` which expects `string`. Asserts that `bar` is echoed.
#[test]
fn test_string_array_element_keeps_string_type() {
    let out = compile_and_run(
        r#"<?php
function paint(string $name): string {
    return $name;
}

function pickSecond(array $names): string {
    return paint($names[1]);
}

echo pickSecond(["foo", "bar"]);
"#,
    );
    assert_eq!(out, "bar");
}

/// Verifies that `string` elements inside a `loadNames(): array` return value retain their type
/// when indexed and passed to a `string`-typed parameter. Asserts that `bar` is echoed.
#[test]
fn test_string_array_return_type_keeps_string_elements() {
    let out = compile_and_run(
        r#"<?php
function paint(string $name): string {
    return $name;
}

function loadNames(): array {
    return ["foo", "bar"];
}

$names = loadNames();
echo paint($names[1]);
"#,
    );
    assert_eq!(out, "bar");
}

/// Verifies that assigning an overflow-promoted value to an undeclared local widens the local,
/// so the inferred return type carries the float instead of re-truncating it at the `return`.
///
/// Before the fix the assignment merge answered with `type_accepts`, which kept `$n` at `int`:
/// PHP's coercive mode lets an `int` accept a `mixed` value, but that acceptance is only paid
/// for by a runtime narrowing at a DECLARED boundary, and an inferred local has none. The
/// narrow local then inferred an `int` return type, and codegen inserted a float-to-int
/// conversion that truncated the promoted value. The seed arrives through a parameter so the
/// shape pins inference, not constant folding.
#[test]
fn test_undeclared_return_keeps_overflow_promotion_through_a_local() {
    let out = compile_and_run(
        r#"<?php
function f(int $seed) { $n = $seed; $n = $n + 1; return $n; }
$r = f(PHP_INT_MAX);
echo $r, "|", gettype($r);
"#,
    );
    assert_eq!(out, "9.2233720368548E+18|double");
}

/// A DECLARED `int` return receiving an overflow-promoted float throws PHP's TypeError
/// instead of silently wrapping to PHP_INT_MIN: the declared boundary runs coercive-mode
/// verification, and a float outside the int range is not coercible.
#[test]
fn test_declared_int_return_overflow_float_throws_type_error() {
    let out = compile_and_run(
        r#"<?php
function f(): int { $n = PHP_INT_MAX; $n = $n + 1; return $n; }
try {
    var_dump(f());
} catch (TypeError $e) {
    echo get_class($e), ":", $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "TypeError:f(): Return value must be of type int, float returned"
    );
}

/// The declared-int return boundary follows PHP's coercive-mode arms for a Mixed value:
/// bool and numeric strings coerce silently, while a non-numeric string, null, and any
/// array throw `TypeError` naming the runtime type. Expected output taken from php -n
/// 8.5.6, not authored.
#[test]
fn test_declared_int_return_mixed_coercion_arms() {
    let out = compile_and_run(
        r#"<?php
function mk(mixed $v) { return $v; }
function r(): int { global $probe; return mk($probe); }
foreach ([true, "5", "x", null, [1]] as $p) {
    $probe = $p;
    try { var_dump(r()); } catch (TypeError $e) { echo "TE: ", $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(5)\nTE: r(): Return value must be of type int, string returned\nTE: r(): Return value must be of type int, null returned\nTE: r(): Return value must be of type int, array returned\n"
    );
}

/// An in-range integral float still passes a declared `int` return silently — the
/// boundary verification only rejects what PHP rejects.
#[test]
fn test_declared_int_return_integral_float_passes() {
    let out = compile_and_run(
        r#"<?php
function mk(mixed $v) { return $v; }
function g(): int { return mk(2.0); }
var_dump(g());
"#,
    );
    assert_eq!(out, "int(2)\n");
}

/// A heterogeneous scalar reassignment widens the local instead of keeping the first
/// type: `$x = 1; $x = 1.5;` really contains a float, and the inferred return must not
/// re-truncate it through an int-typed slot.
#[test]
fn test_heterogeneous_scalar_reassignment_int_then_float() {
    let out = compile_and_run(
        r#"<?php
function f() { $x = 1; $x = 1.5; return $x; }
var_dump(f());
"#,
    );
    assert_eq!(out, "float(1.5)\n");
}

/// The mirror direction: `$x = 1.5; $x = 2;` contains an int, and the inferred return
/// must not convert it back up to float through a float-typed slot. Before the widening
/// fix this direction only LOOKED green when constant propagation happened to bypass the
/// env type.
#[test]
fn test_heterogeneous_scalar_reassignment_float_then_int() {
    let out = compile_and_run(
        r#"<?php
function f() { $x = 1.5; $x = 2; return $x; }
var_dump(f());
"#,
    );
    assert_eq!(out, "int(2)\n");
}

/// The boundary TypeError spells a method's name the way PHP does: `C::m(): Return
/// value must be of type int, float returned`.
#[test]
fn test_declared_int_return_boundary_names_the_method() {
    let out = compile_and_run(
        r#"<?php
class C {
    public function m(): int { $n = PHP_INT_MAX; $n = $n + 1; return $n; }
}
try {
    var_dump((new C())->m());
} catch (TypeError $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "C::m(): Return value must be of type int, float returned");
}

/// Verifies that an if/else merge keeps the possibly-float type of a branch-assigned local:
/// the promoted overflow survives the join instead of being re-truncated by the other
/// branch's narrower int.
#[test]
fn test_branch_merged_local_keeps_overflow_promotion() {
    let out = compile_and_run(
        r#"<?php
function f(int $a) {
    if ($a > 0) {
        $n = $a + 1;
    } else {
        $n = 0;
    }
    return $n;
}
$r = f(PHP_INT_MAX);
echo $r, "|", gettype($r);
"#,
    );
    assert_eq!(out, "9.2233720368548E+18|double");
}
