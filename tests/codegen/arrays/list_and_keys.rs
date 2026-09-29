//! Purpose:
//! Integration tests for the `array_is_list`, `array_key_first`, and `array_key_last` builtins,
//! and for their PHP 8.4 value counterparts `array_first` and `array_last`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries; assertions compare stdout.
//! - Covers indexed arrays (compile-time list shape), associative hashes (runtime walk),
//!   integer/string hash keys, empty containers (null key), and case-insensitive calls.
//! - The `array_first`/`array_last` expected output was copied from PHP 8.5's own output.

use crate::support::*;

// --- array_is_list ---

/// Verifies array_is_list() is true for an indexed array and false for string-keyed
/// and offset-keyed associative arrays.
/// Fixture: a packed indexed array, a string-keyed hash, and a hash whose keys start at 5.
#[test]
fn test_array_is_list_basic() {
    let out = compile_and_run(
        r#"<?php
echo array_is_list([1, 2, 3]) ? "y" : "n";
echo array_is_list(["a" => 1, "b" => 2]) ? "y" : "n";
echo array_is_list([5 => "x", 6 => "y"]) ? "y" : "n";
echo array_is_list([]) ? "y" : "n";
"#,
    );
    assert_eq!(out, "ynny");
}

/// Verifies array_is_list() walks a hash produced by json_decode($s, true): a JSON array
/// decodes to a list-shaped hash (true), a JSON object decodes to a string-keyed hash (false).
/// Fixture: json_decode of a numeric array and of an object, both as associative.
#[test]
fn test_array_is_list_runtime_hash() {
    let out = compile_and_run(
        r#"<?php
$a = json_decode('[10, 20, 30]', true);
echo array_is_list($a) ? "y" : "n";
$b = json_decode('{"x": 1, "y": 2}', true);
echo array_is_list($b) ? "y" : "n";
"#,
    );
    assert_eq!(out, "yn");
}

/// Verifies array_is_list() is callable case-insensitively, matching PHP builtin name rules.
/// Fixture: mixed-case spelling of the builtin over a packed indexed array.
#[test]
fn test_array_is_list_case_insensitive() {
    let out = compile_and_run(r#"<?php echo Array_Is_List([1, 2, 3]) ? "y" : "n";"#);
    assert_eq!(out, "y");
}

// --- array_key_first / array_key_last ---

/// Verifies array_key_first()/array_key_last() return positional integer keys for indexed arrays.
/// Fixture: a three-element indexed array; first key 0, last key 2.
#[test]
fn test_array_key_edge_indexed() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_key_first($a);
echo array_key_last($a);
"#,
    );
    assert_eq!(out, "02");
}

/// Verifies array_key_first()/array_key_last() return string keys in insertion order.
/// Fixture: a string-keyed associative array with three entries.
#[test]
fn test_array_key_edge_assoc_string() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 1, "y" => 2, "z" => 3];
echo array_key_first($m);
echo array_key_last($m);
"#,
    );
    assert_eq!(out, "xz");
}

/// Verifies array_key_first()/array_key_last() return integer keys from an out-of-order hash.
/// Fixture: an integer-keyed associative array inserted as 3, 1, 7; first 3, last 7.
#[test]
fn test_array_key_edge_assoc_int() {
    let out = compile_and_run(
        r#"<?php
$m = [3 => "a", 1 => "b", 7 => "c"];
echo array_key_first($m);
echo array_key_last($m);
"#,
    );
    assert_eq!(out, "37");
}

/// Verifies array_key_first()/array_key_last() return null for an empty array.
/// Fixture: an empty array literal compared strictly against null.
#[test]
fn test_array_key_edge_empty_is_null() {
    let out = compile_and_run(
        r#"<?php
echo (array_key_first([]) === null) ? "first-null" : "first-val";
echo (array_key_last([]) === null) ? "-last-null" : "-last-val";
"#,
    );
    assert_eq!(out, "first-null-last-null");
}

// --- array_first / array_last (PHP 8.4) ---

/// Verifies array_first()/array_last() return the edge VALUES in insertion order.
/// Fixture: int, string, nested, and heterogeneous lists; string-keyed, out-of-order int-keyed,
/// and mixed-keyed hashes; a hash whose head and tail were unset; an empty array (null); a
/// `mixed` return holding a list and a hash; and namespaced/case-insensitive spellings.
/// Expected output copied from `php` 8.5.
#[test]
fn test_array_first_last_values() {
    let out = compile_and_run(
        r#"<?php
function box(bool $assoc): mixed {
    return $assoc ? ["x" => 10, "y" => 20] : [7, 8, 9];
}
$ints = [3, 1, 4];
$strs = ["alpha", "beta", "gamma"];
$nested = [[1, 2], [3, 4, 5]];
$hetero = [1, "two", 3.5];
$assoc = ["one" => 1, "two" => 2, "three" => 3];
$intKeys = [5 => "five", 9 => "nine", 2 => "two"];
$mixedKeys = [10 => "ten", "k" => "kay", 3 => [1]];
$h = ["a" => 1, "b" => 2, "c" => 3];
unset($h["a"], $h["c"]);
echo array_first($ints), " ", array_last($ints), "\n";
echo array_first($strs), " ", array_last($strs), "\n";
echo implode(",", array_first($nested)), " ", implode(",", array_last($nested)), "\n";
echo array_first($hetero), " ", array_last($hetero), "\n";
echo array_first($assoc), " ", array_last($assoc), "\n";
echo array_first($intKeys), " ", array_last($intKeys), "\n";
echo array_first($mixedKeys), " ", count(array_last($mixedKeys)), "\n";
echo array_first($h), " ", array_last($h), "\n";
var_dump(array_first([]), array_last([]));
$m = box($argc > 5);
$n = box($argc < 5);
echo array_first($m), " ", array_last($m), " ", array_first($n), " ", array_last($n), "\n";
echo \ARRAY_FIRST(["z" => "zed"]), " ", Array_Last(["q", "r"]), "\n";
"#,
    );
    assert_eq!(
        out,
        "3 4\nalpha gamma\n1,2 3,4,5\n1 3.5\n1 3\nfive two\nten 1\n2 2\nNULL\nNULL\n7 9 10 20\nzed r\n"
    );
}

/// Verifies array_first()/array_last() release every box they hand back.
/// Fixture: a loop over string lists, nested lists, a mixed-key hash, an empty array, a `mixed`
/// value alternating between a hash and a list, and a temporary `array_map()` result, so a
/// per-call leak or an over-release shows up in the heap-debug summary rather than one total.
/// Expected sum copied from `php` 8.5.
#[test]
fn test_array_first_last_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function box(int $n): mixed {
    if ($n % 2 === 0) {
        return ["x" => "s" . $n, "y" => [$n]];
    }
    return [[$n, $n + 1], "tail" . $n];
}
$acc = 0;
for ($i = 0; $i < 64; $i++) {
    $strs = ["a" . $i, "b" . $i, "c" . $i];
    $nested = [[$i, 1], [2, $i]];
    $assoc = [7 => "seven" . $i, "k" => ["deep" . $i], 3 => $i];
    $m = box($i);
    $f = array_first($strs);
    $l = array_last($strs);
    $nf = array_first($nested);
    $nl = array_last($nested);
    $af = array_first($assoc);
    $al = array_last($assoc);
    $ef = array_first([]);
    $mf = array_first($m);
    $ml = array_last($m);
    $tmp = array_last(array_map(fn($v) => $v . "!", $strs));
    $acc += strlen($f) + strlen($l) + count($nf) + count($nl) + strlen($af) + strlen($al)
        + ($ef === null ? 1 : 0) + strlen($tmp);
    if (is_array($mf)) { $acc += count($mf); }
    if (is_string($ml)) { $acc += strlen($ml); }
}
echo $acc, "\n";
"#,
    );
    assert_eq!(out.stdout, "1737\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "array_first/array_last leaked: {}",
        out.stderr
    );
}

/// A `mixed` argument that holds no array is PHP's run-time TypeError for `array_first()`,
/// `array_last()` and `array_key_first()` (they answered `null` before), naming `false`/`true`
/// for a bool the way php-src does. Hash receivers read the entry through the hash layout helper,
/// so their first and last values are right on main's separate entry storage too.
#[test]
fn test_array_edge_builtins_type_error_and_hash_receivers() {
    let out = compile_and_run(
        r#"<?php
function m(int $n): mixed { return $n > 5 ? [1] : "str"; }
function b(int $n): mixed { return $n > 5; }
try { var_dump(array_first(m($argc))); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { var_dump(array_last(b($argc))); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { var_dump(array_key_first(m($argc))); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump(array_last(["a" => 1, "b" => 2]));
var_dump(array_first([3 => "x", 1 => "y"]));
var_dump(array_first([]), array_last([5, 6]));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array_first(): Argument #1 ($array) must be of type array, string given\n",
            "array_last(): Argument #1 ($array) must be of type array, false given\n",
            "array_key_first(): Argument #1 ($array) must be of type array, string given\n",
            "int(2)\n",
            "string(1) \"x\"\n",
            "NULL\n",
            "int(6)\n",
        )
    );
}

/// A `mixed` holding a box of a box of an array (a hash entry widened by a by-reference
/// `foreach`) reaches the edge helpers unboxed; a resource and a closure are named `resource` and
/// `Closure` in the TypeError; and a caught TypeError releases the rejected operand (the builtins
/// are MAY_THROW, so it is pinned for unwind). Review follow-up for #833.
#[test]
fn test_array_edge_builtins_nested_boxes_and_rejected_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$h = ['k' => [1, 2, 3]];
foreach ($h as &$v) {}
unset($v);
$m = $h['k'];
var_dump(array_first($m), array_last($m), array_key_last($m));
function res(): mixed { return fopen('php://memory', 'r'); }
function clo(): mixed { return fn() => 1; }
try { array_first(res()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { array_last(clo()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
class T {}
function make(): mixed { return new T; }
for ($i = 0; $i < 3; $i++) { try { array_first(make()); } catch (TypeError $e) { echo "caught\n"; } }
"#,
    );
    assert!(out.success, "program exited non-zero: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "int(1)\nint(3)\nint(2)\n",
            "array_first(): Argument #1 ($array) must be of type array, resource given\n",
            "array_last(): Argument #1 ($array) must be of type array, Closure given\n",
            "caught\ncaught\ncaught\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
