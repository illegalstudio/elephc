//! Purpose:
//! Integration tests for the `array_is_list`, `array_key_first`, and `array_key_last` builtins.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries; assertions compare stdout.
//! - Covers indexed arrays (compile-time list shape), associative hashes (runtime walk),
//!   integer/string hash keys, empty containers (null key), and case-insensitive calls.

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

/// The `$a[] = ...` shapes after a negative integer key in a non-empty literal.
///
/// An append inside `eval()` reaches the same runtime `record_insert` branch; it is left out
/// because linking the interpreter under a non-default profile takes most of a minute, which
/// the CI harness times out. These shapes are the same on every PHP version.
const NEGATIVE_KEY_APPEND_SOURCE: &str = r#"<?php
$a = [-5 => "a"];
$a[] = "b";
echo implode(",", array_keys($a)), "\n";
$b = [-5 => "a", "b"];
echo implode(",", array_keys($b)), "\n";
$c = [0 => "z"]; unset($c[0]);
$c[-3] = 1;
$c[] = 2;
echo implode(",", array_keys($c)), "\n";
$d = [-5 => "a", 3 => "b"];
$d[] = "c";
echo implode(",", array_keys($d)), "\n";
function f(array $m): array { $m[] = "x"; return $m; }
echo implode(",", array_keys(f([-9 => 1]))), "\n";
"#;

/// A non-empty literal and a fresh hash continue from a negative key on every PHP version.
///
/// `[-5 => "a"]` then `$a[] = "b"` stores "b" at -4. The same successor is used for a bare
/// literal element, a parameter append, and an append after a later explicit key. Measured
/// on PHP 8.2.33, 8.3.33, 8.4.24, and 8.5.9.
#[test]
fn test_append_after_negative_key_continues_from_it_on_every_profile() {
    let expected = "-5,-4\n-5,-4\n-3,1\n-5,3,4\n-9,-8\n";
    assert_eq!(compile_and_run(NEGATIVE_KEY_APPEND_SOURCE), expected);
    assert_eq!(
        compile_and_run_with_php_version(
            NEGATIVE_KEY_APPEND_SOURCE,
            elephc::php_version::PhpVersion::Php83
        ),
        expected
    );
    assert_eq!(
        compile_and_run_with_php_version(
            NEGATIVE_KEY_APPEND_SOURCE,
            elephc::php_version::PhpVersion::Php82
        ),
        expected
    );
}

/// The empty-array constant is the profile-dependent next index.
///
/// Before PHP 8.3 `[]` and `array()` start at 0, so a negative key does not move the cursor
/// and the following append or `array_push` lands at 0. From 8.3 the constant starts unset
/// and the append continues from the negative key. `array_fill_keys` builds a fresh hash, so
/// it continues on every profile. Measured on PHP 8.2.33 through 8.5.9.
#[test]
fn test_empty_array_negative_key_follows_php_version() {
    let source = r#"<?php
function push(array &$m, $value): void { array_push($m, $value); }
$a = [];
$a[-5] = "a";
$a[] = "b";
echo implode(",", array_keys($a)), "\n";
$b = array();
$b[-5] = "a";
push($b, "b");
echo implode(",", array_keys($b)), "\n";
$c = [];
$c["x"] = 1;
$c[-5] = "a";
$c[] = "b";
echo implode(",", array_keys($c)), "\n";
$u = [];
unset($u[0]);
$u[-5] = "a";
$u[] = "b";
echo implode(",", array_keys($u)), "\n";
$f = array_fill_keys(["-5"], 1);
$f[] = 2;
echo implode(",", array_keys($f)), "\n";
$i = [-5 => "a"];
push($i, "b");
echo implode(",", array_keys($i)), "\n";
$j = [-5 => "a", 3 => "b", "c"];
echo implode(",", array_keys($j)), "\n";
"#;
    let before = "-5,0\n-5,0\nx,-5,0\n-5,0\n-5,-4\n-5,-4\n-5,3,4\n";
    let after = "-5,-4\n-5,-4\nx,-5,-4\n-5,-4\n-5,-4\n-5,-4\n-5,3,4\n";
    assert_eq!(
        compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php82),
        before
    );
    assert_eq!(
        compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php83),
        after
    );
    assert_eq!(compile_and_run(source), after);
}
