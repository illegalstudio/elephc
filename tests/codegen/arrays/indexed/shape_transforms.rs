//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array shape-transform builtins, including fill, pad, and splice.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.
//! - Runtime array_push spreads cover indexed receiver staging, iteration and heap ownership.

use super::*;
use crate::support::compile_and_run_with_heap_debug;

/// Tests `array_fill(start_index, num, value)` — creates a 3-element array indexed from 0,
/// all initialized to 42, then accesses elements via integer index.
#[test]
fn test_array_fill() {
    let out = compile_and_run(
        r#"<?php
$a = array_fill(0, 3, 42);
echo $a[0] . " " . $a[1] . " " . $a[2];
"#,
    );
    assert_eq!(out, "42 42 42");
}

/// Regression: `array_fill` with a STRING value stores every element, not just the first.
/// Strings need 16-byte array slots (pointer + length); the fill previously allocated 8-byte
/// scalar slots, so the 16-byte string writes overflowed and only the first element survived.
/// Routed through the dedicated `__rt_array_fill_str` runtime.
#[test]
fn test_array_fill_string_value() {
    let out = compile_and_run(
        r#"<?php
$x = array_fill(0, 3, "ab");
echo implode(",", $x), "|", count($x), "|", $x[2];
"#,
    );
    assert_eq!(out, "ab,ab,ab|3|ab");
}

/// Tests `array_pad($array, length, value)` — pads `[1, 2]` to length 5 with trailing `0`
/// entries, then verifies the resulting array has exactly 5 elements via `count()`.
#[test]
fn test_array_pad() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = array_pad($a, 5, 0);
echo count($b);
"#,
    );
    assert_eq!(out, "5");
}

/// Tests `array_splice(&$array, offset, length)` — removes 2 elements starting at index 1
/// from `[1, 2, 3, 4, 5]`, captures the removed portion, and verifies both counts.
#[test]
fn test_array_splice() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$removed = array_splice($a, 1, 2);
echo count($removed) . " " . count($a);
"#,
    );
    assert_eq!(out, "2 3");
}

/// Tests `array_combine($keys, $values)` — combines `["a", "b"]` keys with `[1, 2]` values
/// into an associative array, then verifies the resulting array has exactly 2 elements.
#[test]
fn test_array_combine() {
    let out = compile_and_run(
        r#"<?php
$keys = ["a", "b"];
$vals = [1, 2];
$m = array_combine($keys, $vals);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}

/// Tests `array_flip($array)` — inverts values-to-keys on `[10, 20, 30]`, producing a map
/// with 3 entries. Verifies count only.
#[test]
fn test_array_flip() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$f = array_flip($a);
echo count($f);
"#,
    );
    assert_eq!(out, "3");
}

/// Tests `array_flip` integer-value key normalization — flips `[10, 20]`, then accesses
/// flipped keys using both integer (`$f[10]`) and string (`$f["20"]`) index forms, verifying
/// PHP's loose-key comparison for integer-like string keys.
#[test]
fn test_array_flip_integer_values_are_integer_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20];
$f = array_flip($a);
echo $f[10] . "|" . $f["20"];
"#,
    );
    assert_eq!(out, "0|1");
}

/// Tests `array_flip` with string values that normalize to the same integer key — flips
/// `["1", "02", "2"]` where "02" and "2" collide under PHP integer-string key normalization,
/// then verifies the resulting count is 3 and each flipped entry is accessible by its
/// canonical integer key.
#[test]
fn test_array_flip_string_values_normalize_numeric_keys() {
    let out = compile_and_run(
        r#"<?php
$a = ["1", "02", "2"];
$f = array_flip($a);
echo count($f) . "|" . $f[1] . "|" . $f["02"] . "|" . $f["2"];
"#,
    );
    assert_eq!(out, "3|0|1|2");
}

/// Tests `array_chunk($array, size)` — splits `[1, 2, 3, 4, 5]` into chunks of size 2,
/// producing 3 chunks. Verifies chunk count via `count()`.
#[test]
fn test_array_chunk() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = array_chunk($a, 2);
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies `array_chunk()` and `array_merge()` carry their element SHAPE, not just their size.
///
/// Both allocate through the shared array constructor, which leaves the `value_type` lane of
/// the packed kind word empty, and neither stamped it. Every reader then treated the slots as
/// raw words: a heterogeneous source produced the right number of chunks of the right sizes
/// whose elements printed as ADDRESSES, and `implode()` over one of them read those addresses
/// as string pointers and died with SIGBUS.
///
/// The source is heterogeneous on purpose. A `[1,2,3]` source is `array<int>`, whose value_type
/// is the zero the constructor already leaves behind — so the whole defect is invisible unless
/// the elements are boxed. The existing size-only assertion above passed throughout.
///
/// `array_chunk` inherits the tag at RUN time (the helper is shared and only sees the source
/// header), `array_merge` stamps it at emit time from the element type its lowering computed.
#[test]
fn test_chunk_and_merge_carry_the_element_shape_of_a_heterogeneous_source() {
    let out = compile_and_run(
        r#"<?php
$a = [1, "b", 3, 4, 5];
foreach (array_chunk($a, 2) as $i => $c) { echo $i, ":", implode(",", $c), ";"; }
echo "|", implode(",", array_merge($a, ["c", 9]));
"#,
    );
    assert_eq!(out, "0:1,b;1:3,4;2:5;|1,b,3,4,5,c,9");
}

/// Tests `array_fill_keys($keys, value)` — creates an array from `["x", "y"]` as keys,
/// both initialized to `0`, then verifies the resulting associative array has exactly 2 entries.
#[test]
fn test_array_fill_keys() {
    let out = compile_and_run(
        r#"<?php
$keys = ["x", "y"];
$m = array_fill_keys($keys, 0);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}

/// Regression: `array_fill`/`array_chunk`/`array_pad`/`array_splice` must unbox a `Mixed`/`Union`
/// integer argument (start index, chunk size, target size, offset, length) instead of using the
/// boxed heap pointer as a raw int. Each int arg here is read from a heterogeneous (Mixed-valued)
/// associative array. `array_fill` uses an integer fill value to sidestep an unrelated
/// refcounted-fill limitation with string values.
///
/// **Currently ignored** — pre-existing gap on origin/main: `array_fill($m["n"], 3, 7)` routes
/// through `__rt_array_fill_assoc` (because the start is a non-literal-zero int), which
/// stores every slot as a Mixed cell. `implode` over a Mixed-valued hash segfaults because
/// it does not unbox the per-slot Mixed tag. PHP returns a plain int-keyed array
/// (`[2=>7, 3=>7, 4=>7]`) without any boxing. A proper fix needs `__rt_array_fill_assoc`
/// to store scalar values directly (no Mixed box) and only box refcounted values, plus
/// `implode` to unbox Mixed when iterating over a hash. Tracked as a separate
/// `array-fill-assoc-implode` gap.
#[test]
#[ignore = "pre-existing gap: __rt_array_fill_assoc Mixed-boxing + implode Mixed unbox"]
fn test_shape_transforms_unbox_mixed_int_args() {
    let out = compile_and_run(
        r#"<?php
$m = ["n" => 2, "t" => "x"];
$sz = ["v" => 5, "t" => "y"];
$f = implode(",", array_fill($m["n"], 3, 7));
$c = implode(",", array_chunk([1, 2, 3, 4, 5], $m["n"])[2]);
$p = implode(",", array_pad([1, 2], $sz["v"], 0));
$b = [1, 2, 3, 4];
array_splice($b, $m["n"], $m["n"]);
echo $f, "|", $c, "|", $p, "|", implode(",", $b);
"#,
    );
    assert_eq!(out, "7,7,7|5|1,2,0,0,0|1,2");
}

/// Tests `array_count_values()` over a string-valued indexed array — each distinct value
/// becomes a key mapped to its occurrence tally, in first-seen order.
#[test]
fn test_array_count_values_strings() {
    let out = compile_and_run(
        r#"<?php
$r = array_count_values(["a", "b", "a"]);
foreach ($r as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "a=2;b=1;");
}

/// Tests `array_count_values()` over an integer-valued indexed array.
#[test]
fn test_array_count_values_integers() {
    let out = compile_and_run(
        r#"<?php
$r = array_count_values([1, 1, 2, 3, 3, 3]);
foreach ($r as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "1=2;2=1;3=3;");
}

/// Verifies `array_count_values()` resolves case-insensitively, namespaced, and by named argument.
#[test]
fn test_array_count_values_case_insensitive_namespaced_and_named_args() {
    let out = compile_and_run(
        r#"<?php
$r = ARRAY_COUNT_VALUES(["z", "z"]);
$s = \array_count_values(array: [5, 5, 6]);
foreach ($r as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
foreach ($s as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "z=2;|5=2;6=1;");
}

/// Tests `array_count_values()` over an ASSOCIATIVE source: the source KEYS are discarded and
/// the source VALUES become the tally keys.
#[test]
fn test_array_count_values_assoc_source() {
    let out = compile_and_run(
        r#"<?php
$src = ["k1" => "x", "k2" => "y", "k3" => "x"];
$r = array_count_values($src);
foreach ($r as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "x=2;y=1;");
}

/// Verifies PHP's numeric-string key collapsing: `"10"` and `10` share one tally while the
/// non-canonical `"010"` stays a distinct string key.
#[test]
fn test_array_count_values_numeric_string_keys_collapse() {
    let out = compile_and_run(
        r#"<?php
$r = array_count_values(["10", 10, "010"]);
foreach ($r as $k => $v) { echo var_export($k, true), "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "10=2;'010'=1;");
}

/// Verifies `array_count_values()` tallies runtime-built strings, so the call cannot be folded
/// into a literal and the `__rt_array_count_values` lowering is exercised.
#[test]
fn test_array_count_values_runtime_values() {
    let out = compile_and_run(
        r#"<?php
$n = "n" . ($argc - 1);
$r = array_count_values([$n, "n0", "z"]);
foreach ($r as $k => $v) { echo $k, "=", $v, ";"; }
echo "|", count($r);
"#,
    );
    assert_eq!(out, "n0=2;z=1;|2");
}


/// Pins PHP's real `array_push()` signature: `array_push(array &$array, mixed ...$values)`
/// (issue #677).
///
/// The arity was pinned to exactly two arguments, so `array_push($a, 3, 4)` — ordinary PHP —
/// was rejected with `array_push() takes exactly 2 arguments`, and the value-less
/// `array_push($a)` with it. The return type was `Void`, so `$n = array_push($a, 1)` read `NULL`
/// where PHP gives the new element count.
///
/// The matrix walks the value counts, the growth that a multi-value push forces (each append can
/// reach `__rt_array_grow` and relocate the array, so the receiver is republished BETWEEN values
/// rather than once at the end), a bool payload, all five receiver places, and left-to-right
/// argument evaluation.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_push_accepts_phps_full_variadic_signature() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2]; $n = array_push($a, 3, 4); echo implode(",", $a), "|", $n, "\n";
$b = [1];    $n = array_push($b, 2);    echo implode(",", $b), "|", $n, "\n";
$c = [1, 2]; $n = array_push($c);       echo implode(",", $c), "|", $n, "\n";
$d = [];     $n = array_push($d, 1, 2, 3, 4, 5); echo implode(",", $d), "|", $n, "\n";
$g = [1]; array_push($g, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13); echo implode(",", $g), "|", count($g), "\n";
$bo = [true]; $n = array_push($bo, false, true); echo count($bo), ",", ($bo[1] ? 1 : 0), ",", ($bo[2] ? 1 : 0), "|", $n, "\n";
function viaRef(array &$r) { return array_push($r, 8, 9); }
$p = [1]; $n = viaRef($p); echo implode(",", $p), "|", $n, "\n";
class PushBox { public array $items = [1]; }
$box = new PushBox(); $n = array_push($box->items, 2, 3); echo implode(",", $box->items), "|", $n, "\n";
class PushShelf { public static array $items = [1]; }
$n = array_push(PushShelf::$items, 2, 3); echo implode(",", PushShelf::$items), "|", $n, "\n";
$rows = [[1]]; $n = array_push($rows[0], 2, 3); echo implode(",", $rows[0]), "|", $n, "\n";
function sideEffect(int $v): int { echo "eval", $v, " "; return $v; }
$s = []; array_push($s, sideEffect(1), sideEffect(2), sideEffect(3)); echo "| ", implode(",", $s), "\n";
$r = [1]; array_push($r, 2, 3, 4); echo $r[0], $r[1], $r[2], $r[3], "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1,2,3,4|4\n",
            "1,2|2\n",
            "1,2|2\n",
            "1,2,3,4,5|5\n",
            "1,2,3,4,5,6,7,8,9,10,11,12,13|13\n",
            "3,0,1|3\n",
            "1,8,9|3\n",
            "1,2,3|3\n",
            "1,2,3|3\n",
            "1,2,3|3\n",
            "eval1 eval2 eval3 | 1,2,3\n",
            "1234\n",
        )
    );
}

/// A Generator spread into an indexed local appends each yielded value and releases every owner.
#[test]
fn test_array_push_indexed_generator_spread_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function indexedPushGenerator(): Generator { yield 3; yield 4; }
for ($i = 0; $i < 32; $i++) {
    $values = [1, 2];
    $count = array_push($values, ...indexedPushGenerator());
}
echo $count, "|", json_encode($values), "\n";
"#,
    );
    assert_eq!(out.stdout, "4|[1,2,3,4]\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A runtime indexed-list spread appends its elements separately and leaves the heap balanced.
#[test]
fn test_array_push_indexed_runtime_list_spread_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) {
    $values = [1, 2];
    $tail = [$argc > 0 ? 3 : 30, 4];
    $count = array_push($values, ...$tail);
}
echo $count, "|", json_encode($values), "\n";
"#,
    );
    assert_eq!(out.stdout, "4|[1,2,3,4]\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reference aliases accept compatible plain values, while prior builtins do not create aliases.
#[test]
fn test_array_push_indexed_reference_alias_compatible_plain_values_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [1];
$alias =& $values;
$count = array_push($alias, 2, 3);
echo $count, "|", json_encode($values), "|", json_encode($alias), "\n";
$sequential = [1];
array_push($sequential, 2);
echo array_push($sequential, "x"), "|", json_encode($sequential), "\n";
$sorted = [2, 1];
sort($sorted);
echo array_push($sorted, "x"), "|", json_encode($sorted), "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "3|[1,2,3]|[1,2,3]\n3|[1,2,\"x\"]\n3|[1,2,\"x\"]\n",
        "stderr: {}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested indexed receivers under a reference-bound root use copy/write-back without crashing.
#[test]
fn test_array_push_nested_indexed_reference_root_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$rows = [[1], [2]];
$alias =& $rows;
$first = array_push($rows[0], 2);
$second = array_push($rows[1]);
$loopRows = [[1], [2]];
$loopAlias =& $loopRows;
for ($i = 0; $i < 2; $i++) {
    array_push($loopRows[$i], (int) ($i + 10));
}
$hashRows = [["a" => 1]];
$hashAlias =& $hashRows;
$hashCount = array_push($hashRows[0], 2);
echo $first, "|", $second, "|", json_encode($rows), "|", json_encode($alias), "\n";
echo json_encode($loopRows), "|", json_encode($loopAlias), "\n";
echo $hashCount, "|", json_encode($hashRows), "|", json_encode($hashAlias), "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "2|1|[[1,2],[2]]|[[1,2],[2]]\n[[1,10],[2,11]]|[[1,10],[2,11]]\n2|[{\"a\":1,\"0\":2}]|[{\"a\":1,\"0\":2}]\n",
        "stderr: {}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Closure-body mutations keep their boxed capture path while native alias readers stay typed.
#[test]
fn test_array_push_reference_captures_allow_plain_value_widening() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$captured = [1];
$add = function () use (&$captured) { return array_push($captured, "y"); };
echo $add(), "|", json_encode($captured), "\n";
$shared = [1];
$alias =& $shared;
$readShared = function () use (&$shared) { return json_encode($shared); };
array_push($alias, 2);
echo json_encode($alias), "|", $readShared(), "\n";
"#,
    );
    assert_eq!(out.stdout, "2|[1,\"y\"]\n[1,2]|[1,2]\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unrelated read-only array captures retain native storage for typed consumers in both frames.
/// The pre-bound native reference control has the same retained graph, so require no added owner.
#[test]
fn test_array_push_capture_plans_preserve_read_only_array_builtins() {
    let source = r#"<?php
$values = [3, 1, 2];
$read = function () use (&$values) {
    echo min($values), "/", max($values), "|", array_search(1, $values, true), "\n";
    echo implode(",", array_merge($values, [4])), "|";
    echo implode(",", array_diff($values, [1])), "|";
    echo implode(",", $values + [5 => 5]), "\n";
};
$read();
echo min($values), "/", max($values), "|", array_search(1, $values, true), "\n";
echo implode(",", array_merge($values, [4])), "|", implode(",", array_diff($values, [1])), "\n";
$floats = [2.5, 1.5];
$floatRead = function () use (&$floats) { return min($floats) + max($floats) + array_sum($floats); };
echo $floatRead(), "\n";
array_push($values, 10);
echo min($values), "/", max($values), "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    let control = compile_and_run_with_heap_debug(&source
        .replacen("$read =", "$controlAlias =& $values;\n$read =", 1)
        .replacen("$floatRead =", "$floatAlias =& $floats;\n$floatRead =", 1));
    assert_eq!(out.stdout,
        "1/3|1\n3,1,2,4|3,2|3,1,2,5\n1/3|1\n3,1,2,4|3,2\n8\n1/10\n",
        "stderr: {}", out.stderr);
    assert_eq!(control.stdout, out.stdout, "native control: {}", control.stderr);
    let summary = |stderr: &str| stderr.lines()
        .find(|line| line.starts_with("HEAP DEBUG: leak summary:"))
        .expect("heap debug must report final ownership").to_owned();
    assert_eq!(summary(&out.stderr), summary(&control.stderr),
        "candidate: {}\npre-bound native control: {}", out.stderr, control.stderr);
}

/// Nested capture mutations preserve compatible native layouts and outer indexed reads.
#[test]
fn test_array_push_nested_captures_preserve_compatible_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$rows = [[1]];
$push = function () use (&$rows) { array_push($rows[0], 2); };
$push();
echo $rows[0][0], "|", $rows[0][1], "|", json_encode($rows), "\n";
$named = [[[2, 3]]];
$popNamed = function () use (&$named) { return array_pop(array: $named[0][0]); };
echo $popNamed(), "|", $named[0][0][0], "|", json_encode($named), "\n";
"#);
    assert_eq!(out.stdout, "1|2|[[1,2]]\n3|2|[[[2]]]\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Indexed spread appends share the ordinary widening path for every source and value shape.
#[test]
fn test_array_push_indexed_heterogeneous_spreads_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function heterogeneousPushGenerator(): Generator { yield "x"; yield 1.5; }
function emptyPushGenerator(): Generator { yield "g"; yield 2.5; }
$dynamic = [1, 2];
$tail = [$argc > 0 ? "x" : "y", "z"];
echo array_push($dynamic, ...$tail), "|", json_encode($dynamic), "\n";
$literal = [$argc];
echo array_push($literal, ...["a", "b"]), "|", json_encode($literal), "\n";
$generated = [1, 2];
echo array_push($generated, ...heterogeneousPushGenerator()), "|", json_encode($generated), "\n";
$plain = [1];
$more = [$argc + 1];
echo array_push($plain, "s", ...$more), "|", json_encode($plain), "\n";
$empty = [];
$items = [$argc + 1, 2, $argc + 3];
echo array_push($empty, ...$items), "|", $empty[0] + $empty[2], "|", json_encode($empty), "\n";
$emptyGenerated = [];
echo array_push($emptyGenerated, ...emptyPushGenerator()), "|", json_encode($emptyGenerated), "\n";
$strings = ["a"];
echo array_push($strings, ...[$argc + 1, 5]), "|", json_encode($strings), "\n";
"#,
    );
    assert_eq!(out.stdout, concat!(
        "4|[1,2,\"x\",\"z\"]\n",
        "3|[1,\"a\",\"b\"]\n",
        "4|[1,2,\"x\",1.5]\n",
        "3|[1,\"s\",2]\n",
        "3|6|[2,2,4]\n",
        "2|[\"g\",2.5]\n",
        "3|[\"a\",2,5]\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies a multi-value `array_push()` neither leaks the appended values nor double releases
/// the storage its own growth relocated.
///
/// Thirteen appends into a one-element array force several `__rt_array_grow` calls, each of which
/// frees the previous buffer after republishing the new pointer, so an imbalance on either side
/// shows up here rather than in a single-iteration total.
#[test]
fn test_array_push_variadic_growth_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) {
    $g = [1];
    array_push($g, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13);
}
echo count($g), "\n";
"#,
    );
    assert_eq!(out.stdout, "13\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "variadic array_push leaked: {}",
        out.stderr
    );
}

/// Verifies one unique packed array can reach 400,000 integer slots inside the default
/// eight-megabyte heap while heap-debug accounting remains balanced.
///
/// Allocate-copy-free growth leaves every retired bump allocation below the live tail. Before
/// tail extension, the final 262,144-to-524,288 capacity step exhausted the default heap even
/// though the final live array occupies only about four megabytes.
#[test]
fn test_unique_packed_array_tail_growth_fits_default_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [];
for ($i = 0; $i < 400000; $i++) { array_push($values, $i); }
echo count($values), "\n";
"#,
    );
    assert_eq!(out.stdout, "400000\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "tail-grown packed array leaked: {}",
        out.stderr
    );
}

/// Verifies the growth republish through a BY-REFERENCE parameter, where the new pointer has to
/// travel back out to the caller's storage rather than into a local slot.
///
/// `test_array_push_variadic_growth_is_heap_clean` above covers a plain local, whose republish
/// is `store_value_to_raw_local`. A by-reference parameter is a different write: the receiver is
/// a reference cell, loaded with `LoadRefCell` and published with `store_value_to_ref_cell_local`
/// THROUGH the cell pointer into the caller's frame. Nothing pinned that second store — the
/// by-ref fixture in `test_array_push_accepts_phps_full_variadic_signature` pushes two values
/// onto `[1]`, reaching `__rt_array_grow` once, which is not enough to catch a dropped republish
/// (issue #1088).
///
/// That this fixture catches it is measured rather than argued: disabling ONLY the reference-cell
/// arm of `store_value_to_local`, leaving the plain-local arm intact, makes this the single
/// failing test out of the 745 in `arrays::`. Disabling both arms fails six, so the suite covers
/// the local path thoroughly and covered this one not at all. It reaches `__rt_array_grow` 39
/// times, all of them through `__rt_array_push_int` / `_str` — the typed lowering, not the boxed
/// `__rt_mixed_array_append` path, which would republish inside the Mixed cell and pin nothing
/// here.
///
/// The variants are the ways the receiver can be shaped when growth hits: pushed all at once;
/// one per call, which is the hedge against a future bulk-append lowering that would drain the
/// first variant of reallocations while keeping it green; shared with a copy, so growth follows a
/// copy-on-write split and `count($copy)` pins it; a run of string values, where a stale pointer
/// is a use-after-free of the elements rather than only of the buffer; and a forwarding wrapper,
/// so the reference crosses two frames.
///
/// The STDOUT assertion is the load-bearing one. `__rt_array_grow` frees the old buffer right
/// after publishing the new pointer, so a dropped republish is a use-after-free rather than a
/// leak: the caller reads a stale count instead of 33. The heap assertion catches the
/// copy-on-write variant's orphan and any double release, which reports as a fatal before the
/// summary prints.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_push_growth_through_a_by_ref_parameter_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function pushMany(array &$a): int { return array_push($a, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33); }
function pushOne(array &$a, int $v): int { return array_push($a, $v); }
function pushStrings(array &$a): int {
    return array_push($a, "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s");
}
function forward(array &$b): int { return pushMany($b); }
$x = [1];
$n = pushMany($x);
$y = [1];
for ($i = 2; $i < 34; $i++) { pushOne($y, $i); }
$z = [1];
$copy = $z;
$m = pushMany($z);
$s = ["a"];
$k = pushStrings($s);
$f = [1];
$p = forward($f);
echo $n, ",", count($x), ",", $x[32], "|",
     count($y), ",", $y[32], "|",
     $m, ",", count($z), ",", count($copy), "|",
     $k, ",", $s[0], $s[18], "|",
     $p, ",", count($f), "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "33,33,33|33,33|33,33,1|19,as|33,33\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "by-reference array_push growth leaked: {}",
        out.stderr
    );
}

/// Follow-up for #1191: growth republish through the BOXED `Mixed` append path.
///
/// The typed receivers republish a pointer the lowering writes back. A boxed receiver does
/// not: `__rt_mixed_array_append` republishes the container INSIDE the Mixed cell, which is a
/// different mechanism, and #1104's fixtures could not reach it. The ternary is `$argc`-keyed
/// so the merge genuinely produces `Mixed` rather than something the folder can collapse; the
/// harness runs the binary with no arguments, so the first arm is taken.
///
/// Both call shapes are covered, because they reach the append differently: one variadic call
/// with 32 values, and 32 single-value calls.
///
/// The RETURN value is asserted too. It comes from `__rt_mixed_count`, which is on the
/// single-argument int-result ABI rather than the C argument ABI, and reading it through the
/// wrong register made `array_push()` answer 0 here while the container itself was correct.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_push_growth_through_a_boxed_mixed_receiver_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$seed = [[1]];
$m = $argc == 1 ? $seed[0] : [9];
$n = array_push($m, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33);
echo $n, ",", count($m), ",", $m[32], ",", $m[0], "\n";
$o = $argc == 1 ? $seed[0] : [9];
for ($i = 2; $i < 34; $i++) { array_push($o, $i); }
echo count($o), ",", $o[32], "\n";
"#,
    );
    assert_eq!(out.stdout, "33,33,33,1\n33,33\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "boxed Mixed array_push growth leaked: {}",
        out.stderr
    );
}

/// Follow-up for #1190: growth republish through an instance-property and a nested-element
/// receiver, not only through a by-reference parameter.
///
/// #1104 pinned `store_value_to_ref_cell_local` after `__rt_array_grow`. These two receivers
/// republish through different store paths (a property slot and an element slot) and were
/// covered only by the 2-value `viaRef` rows of the signature test, which never reallocate.
/// A dropped republish here is a use-after-free rather than a leak, because
/// `__rt_array_grow` frees the old buffer as soon as it has published the new pointer, so the
/// stdout assertion is the load-bearing one and the heap assertion catches the double release.
///
/// Static-property growth is also fixed. Its count/element and in-loop `implode()` cases
/// have separate heap-clean regressions in `pinned_fixed_issues`:
/// `test_issue_1207_array_push_into_static_property_grows_inside_a_loop` and
/// `test_issue_1207_static_property_array_push_then_implode_in_a_loop`.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_push_growth_through_property_and_element_receivers_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public array $items = [1]; }
$o = new Box();
for ($i = 2; $i < 34; $i++) { array_push($o->items, $i); }
$rows = [[1]];
for ($i = 2; $i < 34; $i++) { array_push($rows[0], $i); }
echo count($o->items), ",", $o->items[32], "|", count($rows[0]), ",", $rows[0][32], "\n";
"#,
    );
    assert_eq!(out.stdout, "33,33|33,33\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "array_push growth through a property or element receiver leaked: {}",
        out.stderr
    );
}

/// Follow-up for #1189: proves `__rt_array_grow` really ran, rather than assuming it did.
///
/// The fixture above asserts post-realloc values and heap cleanliness, neither of which a
/// future pre-size or bulk-append lowering would disturb: it could satisfy every assertion
/// with ZERO reallocations and the growth pin would silently stop pinning growth.
///
/// The side channel is the allocation count. A second live array is allocated after the array
/// under test, deliberately preventing its first growth from extending the bump tail in place.
/// Each program is compared against a control that builds the same arrays and calls a
/// by-reference function that appends nothing. The forced fallback reallocation therefore
/// shows up as an extra `allocs` and nothing else can produce one.
///
/// The two halves are measured separately on purpose. A bulk-append lowering could pre-size
/// the variadic call and leave the one-per-call loop growing, which a combined total would
/// hide.
#[test]
fn test_array_push_growth_through_a_by_ref_parameter_really_reallocates() {
    let allocs = |source: &str, expected_stdout: &str| -> u64 {
        let out = compile_and_run_with_gc_stats(source);
        assert_eq!(out.stdout, expected_stdout, "stderr: {}", out.stderr);
        let stats = out
            .stderr
            .lines()
            .find(|line| line.starts_with("GC: allocs="))
            .unwrap_or_else(|| panic!("no GC stats line in: {}", out.stderr));
        stats
            .trim_start_matches("GC: allocs=")
            .split_once(" frees=")
            .and_then(|(allocs, _)| allocs.parse().ok())
            .unwrap_or_else(|| panic!("unexpected GC stats shape: {stats}"))
    };

    let variadic = allocs(
        r#"<?php
function pushMany(array &$a): int { return array_push($a, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33); }
$x = [1];
$blocker = [99];
$n = pushMany($x);
echo $n, ",", count($x), ",", $x[32], ",", $blocker[0], "\n";
"#,
        "33,33,33,99\n",
    );
    let variadic_control = allocs(
        r#"<?php
function lengthOf(array &$a): int { return count($a); }
$x = [1];
$blocker = [99];
$n = lengthOf($x);
echo $n, ",", count($x), ",", $x[0], ",", $blocker[0], "\n";
"#,
        "1,1,1,99\n",
    );
    assert!(
        variadic > variadic_control,
        "a variadic push from capacity 1 to 33 must reallocate: {variadic} allocs vs \
         {variadic_control} for the same arrays without the push"
    );

    let one_per_call = allocs(
        r#"<?php
function pushOne(array &$a, int $v): int { return array_push($a, $v); }
$y = [1];
$blocker = [99];
for ($i = 2; $i < 34; $i++) { pushOne($y, $i); }
echo count($y), ",", $y[32], ",", $blocker[0], "\n";
"#,
        "33,33,99\n",
    );
    let one_per_call_control = allocs(
        r#"<?php
function lengthOf(array &$a): int { return count($a); }
$y = [1];
$blocker = [99];
for ($i = 2; $i < 34; $i++) { lengthOf($y); }
echo count($y), ",", $y[0], ",", $blocker[0], "\n";
"#,
        "1,1,99\n",
    );
    assert!(
        one_per_call > one_per_call_control,
        "32 single-value pushes must reallocate: {one_per_call} allocs vs \
         {one_per_call_control} for the same loop without the push"
    );
}

/// Verifies a variadic `array_push()` evaluates EVERY value before it appends any of them.
///
/// PHP evaluates a call's arguments and only then enters the function, so nothing an argument
/// reads may observe an append the same call performs: `$a = [10]; array_push($a, 1, count($a));`
/// appends `1`, leaving `[10, 1, 1]`.
///
/// The `ir_lower` fast path for a plain local originally interleaved the two — lower a value,
/// append it, lower the next — which was invisible while the arity was pinned at one value and
/// produced `[10, 1, 2]` as soon as it was not. The general runtime-call path (a property
/// receiver, say) never had the bug, because `lower_builtin_call_args` lowers every operand
/// first; both receiver kinds are covered here so they cannot drift apart again.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_push_evaluates_every_value_before_appending() {
    let out = compile_and_run(
        r#"<?php
$a = [10]; array_push($a, 1, count($a)); echo implode(",", $a), "\n";
$b = [10]; array_push($b, count($b), count($b), count($b)); echo implode(",", $b), "\n";
$c = [1]; array_push($c, $c[0], 99); echo implode(",", $c), "\n";
class PushOrderBox { public array $items = [10]; }
$box = new PushOrderBox(); array_push($box->items, 1, count($box->items)); echo implode(",", $box->items), "\n";
function pushViaRef(array &$r) { array_push($r, 1, count($r)); }
$d = [10]; pushViaRef($d); echo implode(",", $d), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "10,1,1\n",
            "10,1,1,1\n",
            "1,1,99\n",
            "10,1,1\n",
            "10,1,1\n",
        )
    );
}
