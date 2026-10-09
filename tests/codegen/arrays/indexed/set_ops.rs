//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array set-operation builtins, including unique, diff, and intersect.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `array_diff`, `array_intersect` and `array_unique` compare elements by their string
/// rendering over every layout, and keep each survivor's ORIGINAL key, as php does.
///
/// The old helpers compared raw 8-byte slots, which is a POINTER for a boxed element, so boxed
/// elements were refused, and they rebuilt a dense list: `array_diff([1, 2, 3], [2])` answered
/// `[1, 3]` where php answers `[0 => 1, 2 => 3]`. Strings, associative arrays, a declared
/// `array` and objects with `__toString` were refused too. Run in a loop under `--heap-debug`:
/// the renderings, the lookup set and every kept value are released.
#[test]
fn test_value_set_operations_compare_renderings_and_keep_keys() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Tag { public function __construct(public string $n) {} public function __toString(): string { return $this->n; } }
function keys(array $a): string { $o = []; foreach ($a as $k => $v) { $o[] = $k . "=" . $v; } return implode(",", $o); }
function bare_diff(array $a, array $b): array { return array_diff($a, $b); }
function bare_unique(array $a): array { return array_unique($a); }
function run(): string {
    $out = [];
    for ($i = 0; $i < 40; $i++) {
        $suffix = "s" . ($i % 2);
        $out = [
            keys(array_diff([1, 2, 3], [2])),
            keys(array_intersect([1, 2, 3, 4], [4, 2])),
            keys(array_diff(["a", $suffix, "c"], ["c"])),
            keys(array_diff([1, "b", 3, 4], [3, "z"])),
            keys(array_intersect([1, "b", 3, 4], [3, "b"])),
            keys(array_unique([1, "1", 2, "a", "a", 2.0])),
            keys(array_diff(["x" => "a", "y" => $suffix], ["a"])),
            keys(array_intersect(["x" => 1, "y" => 2], ["2", 3])),
            keys(array_unique(["x" => 1, "y" => "1", "z" => true, "w" => 2.5])),
            keys(array_diff([1.0, 2.5, 3], [1, "2.5"])),
            keys(bare_diff(["k" => 1, 2, 3], [3])),
            keys(bare_unique([1, "1", 1.0, "x"])),
            count(array_unique([new Tag("a"), new Tag("b"), new Tag("a")])),
        ];
    }
    return implode("|", $out);
}
echo run();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "0=1,2=3|1=2,3=4|0=a,1=s1|0=1,1=b,3=4|1=b,2=3|0=1,2=2,3=a|y=s1|y=2|x=1,w=2.5|2=3|k=1,0=2|0=1,3=x|2",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies a kept string is the result's own copy: the source array owns its string bytes and
/// frees them on overwrite, so a result sharing them read freed memory and printed nothing.
#[test]
fn test_value_set_operations_copy_kept_strings() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["k" . rand(1, 1), "z" . rand(1, 1)];
$r = array_diff($a, ["z1"]);
$u = array_unique($a);
$a[0] = "changed";
echo json_encode($r), json_encode($u);
$n = [1, 2, 3, 2];
$v = array_unique($n);
$v[] = 9;
$v[1] = 7;
echo json_encode($v), json_encode($n);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r#"["k1"]["k1","z1"][1,7,3,9][1,2,3,2]"#, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies user code run by a rendering can throw, or mutate the operands, without leaking.
///
/// Rendering an object calls its `__toString`, and rendering an array warns, which runs the
/// error handler. A throw from either used to strand the partial result and the rendering set
/// (7 blocks per `array_unique` call); a handler that nulls the variable holding an operand must
/// not free the array under the scan.
#[test]
fn test_value_set_operations_survive_throwing_and_mutating_user_code() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Boom { public function __toString(): string { throw new RuntimeException("boom"); } }
function unique_throws(): string { try { return (string) count(array_unique(["a" . rand(1, 1), new Boom(), "c"])); } catch (RuntimeException $e) { return $e->getMessage(); } }
function diff_throws(): string { try { return (string) count(array_diff(["a", "b"], [new Boom()])); } catch (RuntimeException $e) { return $e->getMessage(); } }
function handler_throws(): string {
    set_error_handler(function () { throw new LogicException("handler"); });
    try { $r = (string) count(array_intersect(["k" => "v" . rand(1, 1), "z" => [1]], ["v1"])); } catch (LogicException $e) { $r = $e->getMessage(); }
    restore_error_handler();
    return $r;
}
function handler_mutates(): string {
    $other = ["x", "y"];
    set_error_handler(function () use (&$other) { $other = null; return true; });
    $r = array_diff([[1], "x"], $other);
    restore_error_handler();
    return json_encode($r);
}
$out = [];
for ($i = 0; $i < 20; $i++) {
    $out = [unique_throws(), diff_throws(), handler_throws(), handler_mutates()];
}
echo implode("|", $out);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "boom|boom|handler|[[1]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies a non-array operand raises a catchable `TypeError` naming the rejected argument.
///
/// The three builtins used to be declared effect-free, so the call could not be observed to
/// throw and the `catch` never ran.
#[test]
fn test_value_set_operations_raise_catchable_type_error() {
    let out = compile_and_run(
        r#"<?php
function diff_of(mixed $m) { try { return array_diff($m, [1]); } catch (TypeError $e) { return $e->getMessage(); } }
function intersect_with(mixed $m) { try { return array_intersect([1], $m); } catch (TypeError $e) { return $e->getMessage(); } }
echo json_encode(diff_of(5)), "|", json_encode(diff_of([1, 2])), "|", json_encode(intersect_with("s"));
"#,
    );
    assert_eq!(
        out,
        r#""array_diff(): Argument #1 ($array) must be of type array"|{"1":2}|"array_intersect(): Argument #2 must be of type array""#
    );
}

/// Verifies `array_unique()` removes duplicate values; count of `[1,2,2,3,3,3]` is 3.
#[test]
fn test_array_unique() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 2, 3, 3, 3];
$b = array_unique($a);
echo count($b);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies `array_diff()` returns values from `$a` not present in `$b`; count of `[1,2,3,4]` vs `[2,4]` is 2.
#[test]
fn test_array_diff() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4];
$c = array_diff($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies `array_intersect()` returns values present in both `$a` and `$b`; count of `[1,2,3,4]` vs `[2,4,6]` is 2.
#[test]
fn test_array_intersect() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4, 6];
$c = array_intersect($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies `array_rand()` returns a valid key/index within the array bounds `[0, 3)`.
#[test]
fn test_array_rand() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$i = array_rand($a);
if ($i >= 0 && $i < 3) { echo "ok"; }
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `shuffle()` permutes all elements without losing any; count stays 5, sum stays 15.
#[test]
fn test_shuffle() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
shuffle($a);
echo count($a);
echo array_sum($a);
"#,
    );
    assert_eq!(out, "515");
}

/// Verifies `array_diff_key()` removes entries by key; count of `["a"=>"1","b"=>"2"]` minus key "a" is 1.
#[test]
fn test_array_diff_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_diff_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

/// Regression: verifies borrowed arrays inside `$src` are not freed when `$src` is unset after `array_diff_key()`.
#[test]
fn test_gc_array_diff_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [1, 2], "drop" => [3, 4]];
$mask = ["drop" => 1];
$filtered = array_diff_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[1];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies `array_intersect_key()` keeps only entries with matching keys; count of `["a"=>"1","b"=>"2"]` intersecting key "a" is 1.
#[test]
fn test_array_intersect_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_intersect_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

/// Regression: verifies borrowed arrays inside `$src` are not freed when `$src` is unset after `array_intersect_key()`.
#[test]
fn test_gc_array_intersect_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [5, 6], "drop" => [7, 8]];
$mask = ["keep" => 1];
$filtered = array_intersect_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[0] . "|" . $saved[1];
"#,
    );
    assert_eq!(out, "5|6");
}

/// `array_diff()`, `array_intersect()`, `array_diff_key()` and `array_intersect_key()` keep each
/// survivor's ORIGINAL key, as PHP does: an indexed first operand yields an integer-keyed hash
/// rather than a renumbered list. Covers int, float, string and numeric-string elements,
/// associative operands on either side, an int operand against a string one (string-cast
/// equality), and `json_encode` (a sparse array encodes as an object). Regression for #1645.
#[test]
fn test_set_operations_keep_the_surviving_keys() {
    let out = compile_and_run(
        r#"<?php
echo json_encode(array_diff([1, 2, 3], [2])), "\n";
echo json_encode(array_diff([1.5, 2.5, 3.5], [2.5])), "\n";
echo json_encode(array_diff(["a", "b", "c", "d"], ["b", "d"])), "\n";
echo json_encode(array_diff(["1", "01", "2"], ["1"])), "\n";
echo json_encode(array_diff([1, 2, 3], ["2"])), "\n";
echo json_encode(array_diff(["x" => 1, "y" => 2, "z" => 3], [2])), "\n";
echo json_encode(array_diff([1, 2, 3], ["k" => 2])), "\n";
echo json_encode(array_intersect([1, 2, 3, 4], [2, 4])), "\n";
echo json_encode(array_intersect(["a", "b", "c"], ["c", "a"])), "\n";
echo json_encode(array_diff_key([10, 20, 30], [1 => 0])), "\n";
echo json_encode(array_intersect_key([10, 20, 30], [0 => 0, 2 => 0])), "\n";
echo json_encode(array_diff([1, 2], [1, 2])), json_encode(array_diff([1, 2], [9])), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "{\"0\":1,\"2\":3}\n",
            "{\"0\":1.5,\"2\":3.5}\n",
            "{\"0\":\"a\",\"2\":\"c\"}\n",
            "{\"1\":\"01\",\"2\":\"2\"}\n",
            "{\"0\":1,\"2\":3}\n",
            "{\"x\":1,\"z\":3}\n",
            "{\"0\":1,\"2\":3}\n",
            "{\"1\":2,\"3\":4}\n",
            "{\"0\":\"a\",\"2\":\"c\"}\n",
            "{\"0\":10,\"2\":30}\n",
            "{\"0\":10,\"2\":30}\n",
            "[][1,2]\n",
        )
    );
}

/// The kept keys are real keys: `$d[2]` reads the survivor, `isset($d[1])` sees the hole, a
/// `foreach` walks the original keys and `array_values()` renumbers them. Regression for #1645.
#[test]
fn test_array_diff_result_is_indexed_by_the_original_keys() {
    let out = compile_and_run(
        r#"<?php
$d = array_diff([1, 2, 3], [2]);
echo $d[2] ?? "missing", "|", isset($d[1]) ? "has1" : "no1", "|", count($d), "\n";
foreach (array_intersect(["p", "q", "r"], ["r", "p"]) as $k => $v) { echo $k, "=", $v, ","; }
echo "\n", json_encode(array_values(array_diff([5, 6, 7, 8], [6, 8]))), "\n";
"#,
    );
    assert_eq!(out, "3|no1|2\n0=p,2=r,\n[5,7]\n");
}

/// The key-preserving set operations leave the heap clean, including string survivors (persisted
/// into the result), string-cast comparisons between an int and a string operand (the casts are
/// freed and the concat scratch rewound), and the key operations whose literal operands used to
/// stay alive (they were not marked as returning fresh storage). Regression for #1645.
#[test]
fn test_key_preserving_set_operations_heap_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$t = 0;
for ($i = 0; $i < 40 + ($argc > 5 ? 1 : 0); $i++) {
    $t += count(array_diff(["a", "b" . $i, "c"], ["c", "x"]));
    $t += count(array_diff([1, 2, 3], ["2", "x" . $i]));
    $t += count(array_intersect(["a", "b" . $i, "c"], ["c", "a"]));
    $t += count(array_diff(["x" => "p" . $i, "y" => "q"], ["q"]));
    $t += count(array_diff_key(["a" . $i, "b", "c"], [1 => 0]));
    $t += count(array_intersect_key([10, 20, 30], [0 => 0, 2 => 0]));
    $t += count(array_diff_key([5 => 1, 6 => 2], [5 => 0]));
}
echo $t, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "480\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
