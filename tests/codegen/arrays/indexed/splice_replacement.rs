//! Purpose:
//! PHP-differential regression tests for `array_splice()`'s fourth parameter, `$replacement`.
//! The AOT backend used to reject the argument outright (`array_splice() takes 2 or 3
//! arguments`); these fixtures pin the window arithmetic, the receiver forms, the value
//! representations, and the heap balance of the insertion path.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected output pins PHP-compatible replacement windows, value types and alias behavior.
//! - The fixtures cover a replacement LONGER than the removed window (which forces
//!   `__rt_array_grow` and therefore a receiver relocation), SHORTER than it, and a pure
//!   insertion (`$length === 0`), plus negative `$offset`/`$length` combinations.
//! - The receiver matrix repeats the five forms that already worked with three arguments —
//!   plain local, instance property, static property, array element, by-reference parameter —
//!   because only the local form is written back by the backend directly; the other four are
//!   rewritten into hidden temporaries by `ir_lower::expr::ref_place_args`, and the
//!   by-reference parameter needs the receiver's ref cell republished after a relocation.
//! - The heap-debug fixtures assert `leak summary: clean` on the scalar and boxing insertion
//!   paths, so an unreleased replacement literal or an unretained inserted payload fails here.

use super::*;
use crate::support::compile_and_run_with_heap_debug;

/// Verifies every `$replacement` window shape matches reference PHP: longer than the removed
/// span, shorter than it, a pure insertion, negative `$offset`, negative `$length`, a bare
/// scalar, `null`, an out-of-range offset on both ends, an omitted `$length`, and `[]`.
#[test]
fn test_array_splice_replacement_windows_match_php() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3,4,5]; $r1 = array_splice($a, 1, 2, [90,91,92]); echo implode(",",$a), "|", implode(",",$r1), "\n";
$b = [1,2,3,4,5]; $r2 = array_splice($b, 1, 3, [90]); echo implode(",",$b), "|", implode(",",$r2), "\n";
$c = [1,2,3,4,5]; $r3 = array_splice($c, 2, 0, [77,78]); echo implode(",",$c), "|", count($r3), "\n";
$d = [1,2,3,4,5]; $r4 = array_splice($d, -2, 1, [55]); echo implode(",",$d), "|", implode(",",$r4), "\n";
$e = [1,2,3,4,5]; $r5 = array_splice($e, 1, -1, [55,56]); echo implode(",",$e), "|", implode(",",$r5), "\n";
$f = [1,2,3,4,5]; $r6 = array_splice($f, 1, 2, 99); echo implode(",",$f), "|", implode(",",$r6), "\n";
$g = [1,2,3,4,5]; $r7 = array_splice($g, 1, 2, null); echo implode(",",$g), "|", implode(",",$r7), "\n";
$h = [10,20,30]; $r8 = array_splice($h, 10, 5, [1,2]); echo implode(",",$h), "|", count($r8), "\n";
$i = [10,20,30]; $r9 = array_splice($i, -10, 5, [1,2]); echo implode(",",$i), "|", implode(",",$r9), "\n";
$j = [10,20,30]; $ra = array_splice($j, 1, null, [7]); echo implode(",",$j), "|", implode(",",$ra), "\n";
$k = [10,20,30]; $rb = array_splice($k, 1, 0, []); echo implode(",",$k), "|", count($rb), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"1,90,91,92,4,5|2,3
1,90,5|2,3,4
1,2,77,78,3,4,5|0
1,2,3,55,5|4
1,55,56,5|2,3,4
1,99,4,5|2,3
1,4,5|2,3
10,20,30,1,2|0
1,2|10,20,30
10,7|20,30
10,20,30|0
"#
    );
}

/// Verifies the five by-reference receiver forms all observe the insertion, including the
/// relocation a growing replacement forces.
///
/// The instance/static property and element forms go through the hidden-temporary rewrite; the
/// by-reference parameter is read with `load_ref_cell` and needs the grown pointer republished
/// through that cell, which is what used to leave the caller reading freed storage.
#[test]
fn test_array_splice_replacement_receiver_forms_match_php() {
    let out = compile_and_run(
        r#"<?php
class Box { public array $items = [1,2,3,4]; public static array $st = [1,2,3,4]; }
function byref(array &$a) { $r = array_splice($a, 1, 2, [7,8,9]); echo implode(",", $r), "|", implode(",", $a), "\n"; }
$loc = [1,2,3,4]; $r1 = array_splice($loc, 1, 2, [7,8,9]); echo implode(",", $r1), "|", implode(",", $loc), "\n";
$b = new Box(); $r2 = array_splice($b->items, 1, 2, [7,8,9]); echo implode(",", $r2), "|", implode(",", $b->items), "\n";
$r3 = array_splice(Box::$st, 1, 2, [7,8,9]); echo implode(",", $r3), "|", implode(",", Box::$st), "\n";
$nested = [[1,2,3,4]]; $r4 = array_splice($nested[0], 1, 2, [7,8,9]); echo implode(",", $r4), "|", implode(",", $nested[0]), "\n";
$p = [1,2,3,4]; byref($p); echo implode(",", $p), "\n";
$named = [1,2,3]; $r5 = array_splice($named, 1, replacement: [9]); echo implode(",",$r5), "|", implode(",", $named), "\n";
$named2 = [1,2,3]; $r6 = array_splice(array: $named2, offset: 1, length: 1, replacement: [8,9]); echo implode(",",$r6), "|", implode(",", $named2), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"2,3|1,7,8,9,4
2,3|1,7,8,9,4
2,3|1,7,8,9,4
2,3|1,7,8,9,4
2,3|1,7,8,9,4
1,7,8,9,4
2,3|1,9
2|1,8,9,3
"#
    );
}

/// Verifies refcounted element payloads survive the insertion in both directions: strings and
/// integers spliced into a heterogeneous receiver, and nested arrays spliced into an array of
/// arrays.
///
/// The heterogeneous receiver stores boxed Mixed cells, so a typed replacement such as
/// `["A","B","C"]` has to be boxed element by element; the array-of-arrays receiver takes the
/// replacement's payloads verbatim and has to retain each one.
#[test]
fn test_array_splice_replacement_refcounted_values_match_php() {
    let out = compile_and_run(
        r#"<?php
$a = [1, "two", 3, "four", 5];
$r1 = array_splice($a, 1, 2, ["A", "B", "C"]);
foreach ($a as $v) { echo $v, ","; } echo "|";
foreach ($r1 as $v) { echo $v, ","; } echo "\n";
$b = [[1,2],[3,4],[5,6]];
$r2 = array_splice($b, 1, 1, [[7,8],[9,10]]);
echo count($b), ":", $b[1][0], ":", $b[2][1], "|", count($r2), ":", $r2[0][0], "\n";
$c = [1, "two", 3];
$r3 = array_splice($c, 1, 1, [10, 20]);
foreach ($c as $v) { echo $v, ","; } echo "|";
foreach ($r3 as $v) { echo $v, ","; } echo "\n";
$e = [1,2,3];
$r4 = array_splice($e, 3, 0, [99]);
echo implode(",", $e), "|", count($r4), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"1,A,B,C,four,5,|two,3,
4:7:10|1:3
1,10,20,3,|two,
1,2,3,99|0
"#
    );
}

/// Verifies the scalar insertion path leaves no live heap blocks.
///
/// The replacement literal is an owned temporary the call has to release, and the growing
/// receiver's previous storage is freed by `__rt_array_grow`, so an unbalanced insertion
/// shows up here as a leak or a double free rather than as a wrong answer.
#[test]
fn test_array_splice_replacement_scalar_insertion_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1,2,3,4];
$removed = array_splice($a, 1, 2, [9,9,9,9,9]);
$b = [1,2,3,4];
$removed2 = array_splice($b, 1, 2, [9]);
$c = [1,2,3,4];
$removed3 = array_splice($c, 1, 2, []);
$repl = [7,8];
$d = [1,2,3,4];
$removed4 = array_splice($d, 0, 1, $repl);
echo count($a), count($removed), count($b), count($removed2), count($c), count($removed3), count($d), count($removed4), count($repl);
"#,
    );
    assert_eq!(out.stdout, "723222512", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the nested-array (refcounted) insertion path retains what it inserts.
///
/// Each inserted payload is owned by the replacement array too, so a missing `__rt_incref`
/// would free the nested arrays while the receiver still references them.
#[test]
fn test_array_splice_replacement_refcounted_insertion_keeps_values_alive() {
    let out = compile_and_run(
        r#"<?php
$repl = [[7,8],[9,10]];
$b = [[1,2],[3,4],[5,6]];
$removed = array_splice($b, 1, 1, $repl);
unset($removed);
echo $b[1][0], ",", $b[2][1], ",", $repl[0][0], ",", count($b);
"#,
    );
    assert_eq!(out, "7,10,7,4");
}

/// Verifies a bare scalar `$replacement` is treated as a one-element array, matching PHP's
/// `(array) $replacement` cast, and that `null` inserts nothing.
///
/// The scalar has to share the receiver's element representation. `array_splice($ints, 1, 1,
/// true)` is a compile error rather than an insertion, because storing a PHP `bool` in an
/// `array<int>` slot would make `var_dump()` report `int(1)` where PHP reports `bool(true)`.
#[test]
fn test_array_splice_scalar_and_null_replacement_match_php() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3,4,5]; array_splice($a, 1, 2, 99); echo implode(",", $a), "\n";
$c = [1,2,3,4,5]; array_splice($c, 1, 2, null); echo implode(",", $c), "\n";
$d = [1.5, 2.5, 3.5]; array_splice($d, 1, 1, 9.5); echo $d[0], ",", $d[1], ",", $d[2], "\n";
$e = [1,2,3]; array_splice($e, 1, 0, 7); echo implode(",", $e), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"1,99,4,5
1,4,5
1.5,9.5,3.5
1,7,2,3
"#
    );
}

/// Verifies a replacement built from overflow-checked arithmetic works on a typed receiver.
///
/// `[$x + 1, $x + 2]` is an `array<mixed>` at the IR level — `ichecked_add` boxes its result —
/// so the values have to be read back out of their Mixed cells before they land in an
/// `array<int>` payload. Storing the cell pointers instead would print addresses.
#[test]
fn test_array_splice_boxed_arithmetic_replacement_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1,2,3,4,5,6,7,8];
for ($i = 0; $i < 3; $i++) {
    $r = array_splice($a, 1, 1, [100 + $i, 200 + $i]);
    echo implode(",", $a), "|", implode(",", $r), "\n";
}
"#,
    );
    assert_eq!(
        out.stdout,
        r#"1,100,200,3,4,5,6,7,8|2
1,101,201,200,3,4,5,6,7,8|100
1,102,202,201,200,3,4,5,6,7,8|101
"#,
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a `$replacement` whose element type differs from the receiver's promotes the
/// receiver to a heterogeneous array, exactly as PHP does.
///
/// PHP has no per-array element type, so `$a = [1,2,3]; array_splice($a, 1, 1, ["x"])` simply
/// leaves `[1, "x", 3]`. elephc types an indexed array at its payload slot, so the promotion has
/// to reach the receiver LOCAL before the call: the slot widens to `array<mixed>` and
/// `__rt_array_to_mixed` re-boxes the live payloads. Every one of these used to be a hard
/// `unsupported EIR backend feature: array_splice replacement PHP type …` compile error.
#[test]
fn test_array_splice_type_changing_replacement_matches_php() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3,4,5]; $r1 = array_splice($a, 1, 2, ["x","y","z"]); echo implode(",",$a), "|", implode(",",$r1), "\n";
$b = [1,2,3]; $r2 = array_splice($b, 1, 1, "solo"); echo implode(",",$b), "|", implode(",",$r2), "\n";
$c = [1,2,3]; $r3 = array_splice($c, 1, 0, [7.5]); echo implode(",",$c), "|", count($r3), "\n";
$d = [1,2,3]; $r4 = array_splice($d, 1, 1, [true]); echo implode(",",$d), "|", implode(",",$r4), "\n";
$e = ["a","b","c"]; $r5 = array_splice($e, 1, 1, [7]); echo implode(",",$e), "|", implode(",",$r5), "\n";
$f = [1.5,2.5,3.5]; $r6 = array_splice($f, 1, 1, ["mid"]); echo implode(",",$f), "|", implode(",",$r6), "\n";
$g = [1,2,3]; $r7 = array_splice($g, 1, 1, 4.5); echo implode(",",$g), "|", implode(",",$r7), "\n";
$h = ["a","b","c"]; $r8 = array_splice($h, 1, 1, 42); echo implode(",",$h), "|", implode(",",$r8), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"1,x,y,z,4,5|2,3
1,solo,3|2
1,7.5,2,3|0
1,1,3|2
a,7,c|b
1.5,mid,3.5|2.5
1,4.5,3|2
a,42,c|b
"#
    );
}

/// Verifies the promoted receiver still reads back as a normal PHP array everywhere else.
///
/// The widening rewrites the slot's storage representation, so a later `print_r`, element read,
/// `count()`, or `foreach` over the same local has to see boxed Mixed cells rather than the raw
/// integer payloads the slot was originally typed for.
#[test]
fn test_array_splice_promoted_receiver_reads_back_as_php_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3,4,5];
array_splice($a, 1, 2, ["x","y","z"]);
print_r($a);
var_dump($a[1]);
echo count($a), "\n";
foreach ($a as $k => $v) { echo $k, "=>", $v, "\n"; }
"#,
    );
    assert_eq!(
        out,
        r#"Array
(
    [0] => 1
    [1] => x
    [2] => y
    [3] => z
    [4] => 4
    [5] => 5
)
string(1) "x"
6
0=>1
1=>x
2=>y
3=>z
4=>4
5=>5
"#
    );
}

/// Declared array reference parameters publish heterogeneous replacements without changing aliases.
#[test]
fn test_array_splice_type_changing_replacement_on_by_ref_parameter_preserves_cow() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(array &$a): void { array_splice($a, 1, 1, ["x"]); }
$x = [1,2,3];
$alias = $x;
f($x);
echo implode(",", $x), "|", implode(",", $alias), "|", gettype($x[1]);
unset($x, $alias);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1,x,3|1,2,3|string", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
