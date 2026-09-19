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
//! - Every expected string in this file is verbatim `LC_ALL=C php` 8.4 output for the same fixture.
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

/// Pins the DOCUMENTED refusal: a type-changing `$replacement` on a receiver whose storage this
/// call cannot retype stays a named compile error rather than becoming a wrong answer.
///
/// A by-reference parameter shares its storage with a caller slot the callee cannot widen, so
/// promoting it would publish boxed `Mixed` cells through a slot the caller still reads as
/// `array<int>`. The diagnostic has to say so, because "use a local" is the actual workaround.
#[test]
fn test_array_splice_type_changing_replacement_on_by_ref_parameter_is_refused() {
    let error = crate::support::compile_source_expect_backend_error(
        r#"<?php
function f(array &$a): void { array_splice($a, 1, 1, ["x"]); }
$x = [1,2,3];
f($x);
echo implode(",", $x);
"#,
    );
    assert!(
        error.contains("array_splice replacement PHP type")
            && error.contains("a by-reference parameter"),
        "expected the named receiver-promotion diagnostic, got: {error}"
    );
}

/// Pins `array_splice($a, …, $a)`: a `$replacement` that IS the receiver inserts what the
/// receiver held BEFORE the removal, not what the removal left behind (issue #676).
///
/// PHP evaluates `$replacement` into its own array first. elephc passed the receiver pointer
/// twice, so the insertion re-read slots the removal had already overwritten and `[1,2,3]` came
/// back as `1,1,1,3` instead of `1,1,2,3,3`.
///
/// The matrix covers the receiver payloads the issue named — `int`, `string`, `float`,
/// refcounted children — plus the shapes where the pointer reuse was easiest to miss: the
/// returned removal window, a negative offset, a pure insertion, a single-element and an empty
/// receiver.
///
/// It then walks the SPELLINGS, because the aliasing is decided on the lowered operands and each
/// of these reaches them differently: named arguments in canonical order, named arguments
/// REORDERED so `$replacement` is not the fourth source argument, a named `$replacement` with
/// `$length` omitted entirely, and a mix of the two.
///
/// Finally the PLACES, because only the plain local writes back directly — a property, a static
/// property and a container element are rewritten into hidden temporaries, a by-reference
/// parameter reaches the caller's cell, and a `&$x` binding puts two differently-numbered slots
/// on one cell.
#[test]
fn test_array_splice_self_replacement_inserts_the_pre_splice_contents() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3]; $r = array_splice($a, 1, 1, $a); echo implode(",",$a), "|", implode(",",$r), "\n";
$s = ["a","b","c"]; array_splice($s, 1, 1, $s); echo implode(",",$s), "\n";
$f = [1.5,2.5,3.5]; array_splice($f, 2, 0, $f); echo count($f), ",", $f[0], ",", $f[2], ",", $f[5], "\n";
$n = [[1],[2]]; array_splice($n, 0, 1, $n); echo count($n), ",", $n[0][0], ",", $n[1][0], ",", $n[2][0], "\n";
$g = [1,2,3,4]; array_splice($g, -2, 1, $g); echo implode(",",$g), "\n";
$o = [7]; array_splice($o, 0, 1, $o); echo implode(",",$o), "\n";
$e = []; array_splice($e, 0, 0, $e); echo count($e), "\n";
function viaRef(array &$r): void { array_splice($r, 1, 1, $r); }
$b = [10,20,30]; viaRef($b); echo implode(",",$b), "\n";
$m = [1,2,3]; array_splice(array: $m, offset: 1, length: 1, replacement: $m); echo implode(",",$m), "\n";
$p = [1,2,3]; array_splice(array: $p, replacement: $p, offset: 1, length: 1); echo implode(",",$p), "\n";
$q = [1,2,3]; array_splice($q, 1, replacement: $q); echo implode(",",$q), "\n";
$t = [1,2,3,4]; array_splice($t, 1, replacement: $t, length: 2); echo implode(",",$t), "\n";
class Box { public array $items = [1,2,3]; }
$box = new Box(); array_splice($box->items, 1, 1, $box->items); echo implode(",",$box->items), "\n";
class Shelf { public static array $items = [1,2,3]; }
array_splice(Shelf::$items, 1, 1, Shelf::$items); echo implode(",",Shelf::$items), "\n";
$rows = [[1,2,3]]; array_splice($rows[0], 1, 1, $rows[0]); echo implode(",",$rows[0]), "\n";
$bind = [1,2,3]; $alias = &$bind; array_splice($bind, 1, 1, $alias); echo implode(",",$bind), "\n";
$unrelated = [1,2,3]; $other = [9,9]; array_splice($unrelated, 1, 1, $other); echo implode(",",$unrelated), "|", implode(",",$other), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"1,1,2,3,3|2
a,a,b,c,c
6,1.5,1.5,3.5
3,1,2,2
1,2,1,2,3,4,4
7
0
10,10,20,30,30
1,1,2,3,3
1,1,2,3,3
1,1,2,3
1,1,2,3,4,4
1,1,2,3,3
1,1,2,3,3
1,1,2,3,3
1,1,2,3,3
1,9,9,3|9,9
"#
    );
}

/// Verifies the snapshot the self-replacement fix inserts is released rather than leaked.
///
/// The copy is a real owned array, so a missing release would accumulate one allocation per
/// splice; the loop makes that visible instead of hiding it in a single-iteration total.
#[test]
fn test_array_splice_self_replacement_snapshot_is_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) { $a = [1,2,3]; array_splice($a, 1, 1, $a); }
echo implode(",", $a), "\n";
"#,
    );
    assert_eq!(out.stdout, "1,1,2,3,3\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "self-replacement snapshot leaked: {}",
        out.stderr
    );
}


/// Verifies a reference REBOUND away from the receiver does not drag the snapshot with it.
///
/// The self-alias decision follows `alias_local_ref_cell` instructions transitively, and those
/// stay in the function after the source rebinds the reference: `$b = &$a; $b = &$x;` leaves the
/// `$b`–`$a` edge behind while `$b`'s slot now holds whatever `$x` holds. The walk is an
/// over-approximation on purpose — a redundant copy is the worst a false positive can cost — but
/// that is only true once both operands are known to be ARRAYS. Without the type guard the
/// rewrite emitted `ArrayCloneShallow` on an integer and the EIR validator rejected the program:
/// `OperandTypeMismatch { expected: "Heap(Array)", actual: I64 }`.
///
/// The second and third rows keep the other two rebinding outcomes honest: a reference rebound to
/// a DIFFERENT array must not be treated as the receiver, and one rebound BACK to the receiver
/// must still be.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_splice_replacement_rebound_away_from_the_receiver_is_not_snapshotted() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3]; $b = &$a; $x = 9; $b = &$x; array_splice($a, 1, 1, $b); echo implode(",",$a), "\n";
$c = [1,2,3]; $d = &$c; $other = [7,8]; $d = &$other; array_splice($c, 1, 1, $d); echo implode(",",$c), "|", implode(",",$other), "\n";
$e = [1,2,3]; $f = &$e; $spare = [0]; $f = &$spare; $f = &$e; array_splice($e, 1, 1, $f); echo implode(",",$e), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1,9,3\n",
            "1,7,8,3|7,8\n",
            "1,1,2,3,3\n",
        )
    );
}
