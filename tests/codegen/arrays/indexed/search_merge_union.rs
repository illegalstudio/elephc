//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array search, merge, and union builtins, including search, search not found is strict false, and search assigned not found is strict false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `array_search` returns the 0-based integer index of the first match.
#[test]
fn test_array_search() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(20, $a);
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies `array_search` returns strict `false` (===) when the value is absent.
#[test]
fn test_array_search_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(99, $a) === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Regression: assigning the result of `array_search` to a variable before comparing
/// must still yield strict `false`, not a falsy zero or empty string.
#[test]
fn test_array_search_assigned_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$result = array_search(99, $a);
echo $result === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Verifies that `array_search` returns index `0` (not `false`) when the target is
/// at the first position, and that `=== false` correctly distinguishes the two.
#[test]
fn test_array_search_zero_index_is_not_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(10, $a) === false ? "miss" : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

/// Verifies `array_key_exists` returns true for an existing integer key and false for a missing key.
#[test]
fn test_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
if (array_key_exists(1, $a)) { echo "yes"; }
if (!array_key_exists(5, $a)) { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

/// Verifies `array_merge` concatenates two indexed arrays and preserves all elements.
#[test]
fn test_array_merge() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo $c[0] . $c[1] . $c[2] . $c[3];
"#,
    );
    assert_eq!(out, "41234");
}

/// Verifies `array_merge` uses the right operand element type when the left array is empty.
#[test]
fn test_array_merge_empty_left_uses_right_element_type() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo ":";
echo $c[0] . $c[1];
"#,
    );
    assert_eq!(out, "2:34");
}

/// Verifies the `+` operator keeps the left operand's values when both arrays
/// have the same numeric key (left wins semantics).
#[test]
fn test_indexed_array_union_keeps_left_duplicate_numeric_keys() {
    let out = compile_and_run(
        r#"<?php
$left = [10, 20];
$right = [99, 88, 77];
$result = $left + $right;
echo count($result) . ":" . $result[0] . "," . $result[1] . "," . $result[2];
"#,
    );
    assert_eq!(out, "3:10,20,77");
}

/// Verifies the `+` operator appends right-side string-keyed values that do not
/// exist in the left array (right-side keys are preserved for non-conflicting entries).
#[test]
fn test_indexed_array_union_string_values_append_missing_suffix() {
    let out = compile_and_run(
        r#"<?php
$left = ["left"];
$right = ["ignored", "added"];
$result = $left + $right;
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:left,added");
}

/// Verifies that an empty left array combined with `+` produces a result whose
/// indices and count mirror the right operand.
#[test]
fn test_indexed_array_union_empty_left_copies_right_layout() {
    let out = compile_and_run(
        r#"<?php
$result = [] + ["first", "second"];
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:first,second");
}

/// Verifies `array_search` over mixed elements, a mixed-valued hash and a bare `array`.
///
/// Each was refused at compile time — "array_search needle PHP type Str for indexed-array element
/// PHP type Mixed", or "second argument must be array" for a bare `array` — although `in_array`
/// already scanned the same shapes. The search now uses `in_array`'s boxed scan, keeping each
/// element's key, so `$strict` is honoured at run time (`1` loosely matches `"1"` and `true`,
/// strictly neither) and a string key comes back as a string. The loop runs inside a function so
/// a leaked key box or result shows as live blocks at exit.
#[test]
fn test_array_search_over_mixed_elements_and_bare_arrays() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function show(mixed $r): string { return var_export($r, true); }
function bare(array $a, mixed $needle): string { return show(array_search($needle, $a)); }
function run(): string {
    $mixedList = [3, "a", 2.5, "1", true, null];
    $mixedMap = ["x" => 1, "y" => "b", "z" => 2.5];
    $out = [];
    for ($i = 0; $i < 40; $i++) {
        $out = [
            show(array_search("a", $mixedList)),
            show(array_search(1, $mixedList)),
            show(array_search(1, $mixedList, true)),
            show(array_search("1", $mixedList, true)),
            show(array_search(2.5, $mixedList)),
            show(array_search("b", $mixedMap)),
            show(array_search(2.5, $mixedMap, true)),
            show(array_search("q", $mixedMap)),
            bare(["k" => "v", "w" => 7], 7),
            bare([10, 20, 30], 30),
            bare(["s" => "t"], "nope"),
        ];
    }
    return implode("|", $out);
}
echo run();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1|3|false|3|2|'y'|'z'|false|'w'|2|false", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies a written `$strict` no longer leaks the haystack of `array_search` and `in_array`.
///
/// Evaluating a later argument roots the earlier ones, so with `$strict` written the call holds an
/// OWNED reference to the haystack. Both builtins sat in the default "may alias its arguments"
/// result bucket, which keeps such an operand alive, and nothing released it: one reference to the
/// whole haystack leaked per call, on typed arrays too — `origin/main` showed 7 live blocks here.
#[test]
fn test_strict_array_search_and_in_array_release_their_haystack() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function run(): string {
    $l = [10, 20, 30];
    $m = ["x" => "a", "y" => "b"];
    $o = "";
    for ($i = 0; $i < 40; $i++) {
        $o = var_export(array_search(20, $l, true), true) . var_export(array_search("b", $m, true), true)
            . var_export(in_array(20, $l, true), true) . var_export(in_array("q", $m, true), true);
    }
    return $o;
}
echo run();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1'y'truefalse", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
