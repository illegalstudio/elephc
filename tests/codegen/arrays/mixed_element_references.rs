//! Purpose:
//! Regression tests for element references taken through a BOXED receiver: a `mixed`
//! parameter or local, or a declared `array` parameter (boxed on this branch so its
//! representation can change without retyping the caller's slot).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PHP separates an array the moment one of its elements becomes a reference, so an alias
//!   taken BEFORE `$r = &$a[...]` must keep the original value. The boxed lowering rewrote the
//!   shared cell in place and every alias saw the reference and the writes through it.
//! - The packed case additionally promotes the payload to a hash; the promoted hash is now
//!   published into the cell before the packed owner is released, so heap debug stays clean
//!   and nothing observes the cell pointing at freed storage.
//! - Every expected value is real `php` 8.x output for the same fixture.

use crate::support::*;

/// An alias of a `mixed` hash must not observe a reference taken through the original.
#[test]
fn test_element_reference_through_mixed_hash_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
function f(mixed $a): void {
    $b = $a;
    $r = &$a["x"];
    $r = 2;
    echo $b["x"], "|", $a["x"], "|", count($b), "|", count($a), "\n";
}
f(["x" => 1]);
"#,
    );
    assert_eq!(out, "1|2|1|1\n");
}

/// The packed spelling: the reference promotes the payload to a hash, which must land in a
/// cell the alias does not share.
#[test]
fn test_element_reference_through_mixed_packed_array_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
function g(mixed $a): void {
    $b = $a;
    $r = &$a[0];
    $r = 9;
    echo $b[0], "|", $a[0], "|", count($b), "|", count($a), "\n";
}
g([1, 2]);
"#,
    );
    assert_eq!(out, "1|9|2|2\n");
}

/// A declared `array` parameter is boxed too, by value and by reference; both must separate
/// from a local alias, and the by-reference form must still reach the caller.
#[test]
fn test_element_reference_through_declared_array_parameters_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
function h(array $a): void { $b = $a; $r = &$a["x"]; $r = 2; echo $b["x"], "|", $a["x"], "\n"; }
h(["x" => 1]);
function p(array $a): void { $b = $a; $r = &$a[0]; $r = 9; echo $b[0], "|", $a[0], "\n"; }
p([1, 2]);
function rf(array &$a): void { $b = $a; $r = &$a["x"]; $r = 2; echo $b["x"], "|", $a["x"], "\n"; }
$q = ["x" => 1]; rf($q); echo $q["x"], "\n";
"#,
    );
    assert_eq!(out, "1|2\n1|9\n1|2\n2\n");
}

/// `array_shift()` and `array_unshift()` preserve the surviving reference cells.
#[test]
fn test_reference_cells_survive_shift_and_unshift() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$r = &$a[1];
$shifted = array_shift($a);
$r = 99;
echo $shifted, "|", $a[0], "|", $a[1], "\n";
$b = [1, 2];
$s = &$b[0];
array_unshift($b, 0);
$s = 42;
echo $b[0], "|", $b[1], "|", $b[2], "\n";
$c = [1, 2, 3];
$t = &$c[0];
$reversed = array_reverse($c);
$t = 7;
echo $reversed[2], "|", $reversed[0], "\n";
$d = [1, 2];
$u = &$d[0];
array_unshift($d);
$u = 5;
echo $d[0], "\n";
$f = [1, 2];
$x = &$f[1];
$merged = array_merge($f, [3]);
$x = 9;
echo $merged[1], "\n";
"#,
    );
    assert_eq!(out, "1|99|3\n0|42|2\n7|3\n5\n9\n");
}

/// Reference array literals create undefined variables silently, like PHP.
#[test]
fn test_reference_literals_create_undefined_variables_silently() {
    let output = compile_and_run_capture(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo "WARN:", $message, "\n";
    return true;
});
$refs = [&$third];
var_dump($third);
"#,
    );
    assert_eq!(output.stdout, "NULL\n");
    assert!(
        !output.stderr.contains("Undefined variable"),
        "stderr: {:?}",
        output.stderr
    );
}

/// A receiver that shares nothing must not be copied: the reference still reaches the only
/// cell, a missing key is created in it, and a non-array `mixed` keeps answering no element.
#[test]
fn test_element_reference_through_unshared_mixed_keeps_the_single_cell() {
    let out = compile_and_run(
        r#"<?php
function f(mixed $a): void {
    $r = &$a["new"];
    $r = 5;
    echo count($a), "|", $a["new"], "\n";
}
f(["x" => 1]);
function n(mixed $a): void {
    $r = &$a["x"];
    echo var_export($r, true), "\n";
}
n(null);
"#,
    );
    assert_eq!(out, "2|5\nNULL\n");
}

/// The separation adds one cell and the promotion replaces one payload; both must be balanced
/// on every receiver kind, including the by-reference parameter whose store-back retires the
/// previous cell.
#[test]
fn test_element_reference_through_boxed_receivers_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(mixed $a): int { $b = $a; $r = &$a["x"]; $r = 2; return $b["x"] + $a["x"]; }
function g(mixed $a): int { $b = $a; $r = &$a[0]; $r = 9; return $b[0] + $a[0]; }
function rf(array &$a): void { $b = $a; $r = &$a[1]; $r = 7; }
$t = 0;
for ($i = 0; $i < 3; $i++) {
    $t += f(["x" => 1]) + g([1, 2]);
    $q = [1, 2, 3]; rf($q); $t += $q[1];
}
echo $t, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "60\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A reference-element literal must describe its hash-of-Mixed storage. Before the literal
/// widened to `AssocArray{Mixed, Mixed}`, `array_values()` inferred `array<int>` and printed
/// the boxed cell pointer as an integer. elephc extracts the aliased value instead of the
/// reference (PHP keeps the cell and prints `&int(2)`), so the snapshot keeps the value the
/// alias held when the array was read; the corruption this guards against printed a pointer.
#[test]
fn test_array_values_over_reference_element_literal_keeps_the_scalar_payload() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = [&$x];
$b = array_values($a);
$x = 2;
var_dump($b);
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  int(1)\n}\n");
}

/// The associative literal spelling widens the same way.
#[test]
fn test_array_values_over_assoc_reference_element_literal_keeps_the_scalar_payload() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = ["k" => &$x];
$b = array_values($a);
var_dump($b);
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  int(1)\n}\n");
}
