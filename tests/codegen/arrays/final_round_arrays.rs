//! Purpose:
//! Regression tests for the final-round array/lowering review findings (batch 1).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output is `php -n` 8.5's own output for the same fixture.

use crate::support::*;

/// `$GLOBALS['x'][] = v` inside a function appends to the global array instead of replacing it
/// with a fresh one (the alias has no local slot, so the append seeded an empty array first).
#[test]
fn test_globals_alias_append_keeps_existing_elements() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$x = ['a', 'b'];
$n = null;
function f() { $GLOBALS['x'][] = 'c'; $GLOBALS['x'][0] = 'z'; echo count($GLOBALS['x']), "\n"; }
f();
echo implode(",", $x), "\n";
function h() { $GLOBALS['n'][] = 'q'; }
h();
echo json_encode($n), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3\nz,b,c\n[\"q\"]\n");
}

/// `unset($a[$k][$j])` over a nested typed array shortens the child into a sparse hash; the
/// parent used to be typed `array<array<int>>` and the write-back was refused at codegen.
#[test]
fn test_nested_unset_over_typed_child_arrays() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$dp = [[1,2],[3,4]]; unset($dp[1][0]); unset($dp[0][5]); unset($dp[9][1]);
echo json_encode($dp), "\n";
$h = ['a' => [1,2], 'b' => ['x' => 1]]; unset($h['a'][0]); unset($h['b']['x']);
echo json_encode($h), "\n";
$ao = [new ArrayObject([1,2,3]), 5]; unset($ao[0][1]);
echo count($ao[0]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "[[1,2],{\"1\":4}]\n{\"a\":{\"1\":2},\"b\":[]}\n2\n"
    );
}

/// A boxed `mixed` receiver's nested unset linked `Phar::offsetUnset` without its archive
/// library; the non-array child path now answers through the runtime `OffsetUnset` helper.
#[test]
fn test_nested_unset_over_mixed_receiver_links() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function g(mixed $m) { unset($m[1][0]); return $m; }
echo json_encode(g([[1,2],[3,4]])), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[[1,2],{\"1\":4}]\n");
}

/// A named `str_replace()` / `str_ireplace()` call binds by name (no stray fourth operand), and
/// `str_ireplace()` fills its by-reference `$count` as php does.
#[test]
fn test_named_str_replace_and_ireplace_count() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo str_replace(subject: "aab", search: "b", replace: "c"), "\n";
echo implode(",", str_replace(subject: ["aab", "bb"], search: "b", replace: "c")), "\n";
echo str_ireplace(subject: "aBb", search: "b", replace: "c"), "\n";
echo str_ireplace("B", "c", "aBb", $n), $n, "\n";
echo str_replace(search: "b", replace: "c", subject: "aab", count: $m), $m, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "aac\naac,cc\nacc\nacc2\naac1\n");
}

/// `array_merge_recursive()` appends the lists of a shared key (php renumbers them) instead of
/// nesting them.
#[test]
fn test_array_merge_recursive_merges_nested_lists() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo json_encode(array_merge_recursive([1, 2], [3])), "\n";
echo json_encode(array_merge_recursive(['a' => [1, 2]], ['a' => [3]])), "\n";
echo json_encode(array_merge_recursive(['a' => 1], ['a' => 2], ['b' => 'x'])), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[1,2,3]\n{\"a\":[1,2,3]}\n{\"a\":[1,2],\"b\":\"x\"}\n");
}

/// `print_r()` and `var_dump()` over a variadic slot that collected string-keyed named
/// arguments walk the hash (the slot is typed `iterable`; they used to print an empty body).
#[test]
fn test_print_r_and_var_dump_of_named_variadic_tail() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(...$rest) { print_r($rest); var_dump($rest); }
f(1, x: "s");
f(y: 2);
function g(iterable $it) { print_r($it); var_dump($it); }
g([1, 2]);
g(new ArrayIterator([5]));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(out.stdout.starts_with("Array\n(\n    [0] => 1\n    [x] => s\n)\narray(2) {\n  [0]=>\n  int(1)\n  [\"x\"]=>\n  string(1) \"s\"\n}\n"), "{}", out.stdout);
    assert!(out.stdout.contains("ArrayIterator Object"), "{}", out.stdout);
}
