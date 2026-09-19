//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of the element type an array
//! literal stamps on a builtin call, covering array-returning and bool-returning builtins in both
//! the indexed and the associative literal.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Pins issue #1096's own repro: two `array_slice()` calls in an indexed literal stay arrays.
///
/// The element type came from `infer_expr_type_syntactic`, whose builtin allowlist does not
/// name `array_slice`, so it answered `Int`; the literal was stamped `array<int>` and lowering
/// cast each element to match. `(int)` of a non-empty array is `1`, so both elements read back
/// as `int(1)` with no warning.
#[test]
fn test_indexed_literal_of_array_returning_builtins_keeps_the_arrays() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = [array_slice($a, 0, 2), array_slice($a, 2)];
echo count($c[0]), count($c[1]), "|", $c[0][0], $c[0][1], "|", $c[1][0], $c[1][1], $c[1][2];
"#,
    );
    assert_eq!(out, "23|12|345");
}

/// A single array-returning builtin, with no second element to widen the merge and hide it.
///
/// The elements are read by index and by `foreach` rather than through `implode()`, which answers
/// empty for an array of unboxed integers reached through a `Mixed` element — a pre-existing gap
/// that has nothing to do with the stamp (`$c = [$a, "x"]; implode(",", $c[0])` and
/// `function pick(): mixed { return [1, 2, 3]; } implode(",", pick())` both answer empty on
/// stock main). The `explode()` test below imploded a STRING array, which is the half that works.
#[test]
fn test_indexed_literal_of_one_array_returning_builtin() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$c = [array_values($a)];
$sum = 0;
foreach ($c[0] as $v) { $sum = $sum + $v; }
echo count($c[0]), ":", $c[0][0], $c[0][2], ":", $sum;
"#,
    );
    assert_eq!(out, "3:13:6");
}

/// `explode()` is in neither function map either, and its result is a `string` array rather
/// than an `int` one -- the cast reached it the same way.
#[test]
fn test_indexed_literal_of_an_explode_result() {
    let out = compile_and_run(
        r#"<?php
$c = [explode(",", "a,b,c")];
echo implode("|", $c[0]);
"#,
    );
    assert_eq!(out, "a|b|c");
}

/// The associative twin of the same arm: the value type of `["k" => array_slice(...)]`.
#[test]
fn test_assoc_literal_value_from_an_array_returning_builtin() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$c = ["k" => array_slice($a, 0, 2)];
echo count($c["k"]), ":", $c["k"][0], $c["k"][1];
"#,
    );
    assert_eq!(out, "2:12");
}

/// A `bool`-returning builtin took the same `Int` fallback, and `(int)true` is `1` -- so the
/// element came back as `int(1)` rather than `bool(true)`. Unlike the array case nothing
/// downstream refuses it, so this one was silent all the way to the output.
#[test]
fn test_indexed_literal_of_a_bool_returning_builtin_stays_bool() {
    let out = compile_and_run(
        r#"<?php
$c = [in_array(1, [1, 2]), str_contains("abc", "z"), is_array([1])];
var_dump($c[0], $c[1], $c[2]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\n");
}

/// The associative twin of the bool case.
#[test]
fn test_assoc_literal_of_a_bool_returning_builtin_stays_bool() {
    let out = compile_and_run(
        r#"<?php
$c = ["yes" => array_key_exists("a", ["a" => 1]), "no" => array_key_exists("z", ["a" => 1])];
var_dump($c["yes"], $c["no"]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
}

/// A `foreach` over such a literal bound `int(1)` as the value, so the loop body saw scalars
/// where PHP sees arrays.
///
/// The elements are read by index rather than through `implode()`, which segfaults on a `foreach`
/// value bound from ANY nested array literal — `foreach ([[1, 2], [3, 4]] as $p) implode(",", $p)`
/// crashes with no builtin call anywhere in it. That is #1081, whose report blames
/// `array_chunk()`; it is not about `array_chunk()`. Unrelated to the element type, and not
/// something this fix makes better or worse.
#[test]
fn test_foreach_over_a_literal_of_builtin_arrays() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
foreach ([array_slice($a, 0, 2), array_slice($a, 2)] as $part) {
    echo count($part), ":", $part[0], $part[1], ";";
}
"#,
    );
    assert_eq!(out, "2:12;2:34;");
}

/// The control the bug never touched: a USER function returning `array` is found in
/// `ctx.functions` before the syntactic fallback is reached, so its literal was always right.
/// Pinned so a future change to the builtin arm cannot quietly take this path with it.
#[test]
fn test_user_function_element_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
function mk(int $n): array { return [$n, $n + 1]; }
$c = [mk(1), mk(5)];
echo count($c[0]), count($c[1]), "|", $c[0][0], $c[1][0];
"#,
    );
    assert_eq!(out, "22|15");
}

/// The other control: a builtin the syntactic allowlist DOES name keeps answering from it, so
/// the new lookup cannot be the thing that makes these work.
#[test]
fn test_allowlisted_builtin_elements_are_unchanged() {
    let out = compile_and_run(
        r#"<?php
$c = [strtoupper("ab"), strtolower("CD")];
$n = [count([1, 2, 3]), strlen("abcd")];
$f = [sqrt(4.0)];
echo $c[0], $c[1], "|", $n[0], $n[1], "|", $f[0];
"#,
    );
    assert_eq!(out, "ABcd|34|2");
}
