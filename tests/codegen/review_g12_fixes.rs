//! Purpose:
//! Regression tests for the verified review findings of the preg / strtotime / enum_exists /
//! class-relation runtime helpers. Every expected output is `php -n` 8.5's own output.
//!
//! Called from:
//! - `cargo test` through Rust's test harness; the Linux x86_64 shard runs them on x86_64.
//!
//! Key details:
//! - The preg capture tests RUN the program, so on `linux-x86_64` they execute the x86_64
//!   emitters (a register mix-up there once made `$matches['MARK']` and offset-capture text
//!   garbage while the only existing test merely checked emitted labels).

use crate::support::*;

/// The MARK value, offset-capture text and named/numeric captures carry the matched bytes on every target.
#[test]
fn test_preg_match_marks_and_offset_captures_hold_their_text() {
    let out = compile_and_run_capture_with_regex(r####"<?php
preg_match('/b/', 'abc', $m, PREG_OFFSET_CAPTURE); var_dump($m);
preg_match('{^/a(*:12)}', '/a', $m); var_dump($m);
preg_match('/(?<y>\d)-(\d)/', '1-2', $m); var_dump($m);
preg_match_all('/(?<y>\d)-(\d)/', '1-2', $m, PREG_SET_ORDER); var_dump($m);
preg_match('/(?<y>b)/', 'abc', $m, PREG_OFFSET_CAPTURE); var_dump($m);
"####);
    assert_eq!(out.stdout, r####"array(1) {
  [0]=>
  array(2) {
    [0]=>
    string(1) "b"
    [1]=>
    int(1)
  }
}
array(2) {
  [0]=>
  string(2) "/a"
  ["MARK"]=>
  string(2) "12"
}
array(4) {
  [0]=>
  string(3) "1-2"
  ["y"]=>
  string(1) "1"
  [1]=>
  string(1) "1"
  [2]=>
  string(1) "2"
}
array(1) {
  [0]=>
  array(4) {
    [0]=>
    string(3) "1-2"
    ["y"]=>
    string(1) "1"
    [1]=>
    string(1) "1"
    [2]=>
    string(1) "2"
  }
}
array(3) {
  [0]=>
  array(2) {
    [0]=>
    string(1) "b"
    [1]=>
    int(1)
  }
  ["y"]=>
  array(2) {
    [0]=>
    string(1) "b"
    [1]=>
    int(1)
  }
  [1]=>
  array(2) {
    [0]=>
    string(1) "b"
    [1]=>
    int(1)
  }
}
"####, "stderr: {}", out.stderr);
}

/// Out-of-range start offsets, lookbehind at the subject start and PREG_UNMATCHED_AS_NULL with offsets follow php.
#[test]
fn test_preg_match_offset_clamp_notbol_and_unmatched_as_null() {
    let out = compile_and_run_capture_with_regex(r####"<?php
var_dump(preg_match('/a/', 'abc', $m, 0, -10), $m);
var_dump(preg_match('/a/', 'abc', $m, 0, 10), preg_last_error());
var_dump(preg_match('/(?<=^a)b/', 'ab', $m, 0, 1));
var_dump(preg_match('/(a)(b)?(c)?/', 'a', $m, PREG_OFFSET_CAPTURE | PREG_UNMATCHED_AS_NULL), $m);
var_dump(preg_match('/a/', 'abc', $m, 0, 3), preg_last_error());
"####);
    assert_eq!(out.stdout, r####"int(1)
array(1) {
  [0]=>
  string(1) "a"
}
bool(false)
int(1)
int(1)
int(1)
array(4) {
  [0]=>
  array(2) {
    [0]=>
    string(1) "a"
    [1]=>
    int(0)
  }
  [1]=>
  array(2) {
    [0]=>
    string(1) "a"
    [1]=>
    int(0)
  }
  [2]=>
  array(2) {
    [0]=>
    NULL
    [1]=>
    int(-1)
  }
  [3]=>
  array(2) {
    [0]=>
    NULL
    [1]=>
    int(-1)
  }
}
int(0)
int(0)
"####, "stderr: {}", out.stderr);
}

/// An empty separator is retried non-empty at the same offset before the search steps a character.
#[test]
fn test_preg_split_retries_a_non_empty_separator_after_an_empty_one() {
    let out = compile_and_run_capture_with_regex(r####"<?php
var_dump(preg_split('/\b|\s/', 'a b', -1, PREG_SPLIT_NO_EMPTY));
var_dump(preg_split('/\b|\s/', 'a b'));
var_dump(preg_split('/ ??/', 'a b'));
var_dump(preg_split('//', 'abc'));
var_dump(preg_split('//u', 'aé'));
var_dump(preg_split('/x*/', 'axb'));
"####);
    assert_eq!(out.stdout, r####"array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
}
array(6) {
  [0]=>
  string(0) ""
  [1]=>
  string(1) "a"
  [2]=>
  string(0) ""
  [3]=>
  string(0) ""
  [4]=>
  string(1) "b"
  [5]=>
  string(0) ""
}
array(6) {
  [0]=>
  string(0) ""
  [1]=>
  string(1) "a"
  [2]=>
  string(0) ""
  [3]=>
  string(0) ""
  [4]=>
  string(1) "b"
  [5]=>
  string(0) ""
}
array(5) {
  [0]=>
  string(0) ""
  [1]=>
  string(1) "a"
  [2]=>
  string(1) "b"
  [3]=>
  string(1) "c"
  [4]=>
  string(0) ""
}
array(4) {
  [0]=>
  string(0) ""
  [1]=>
  string(1) "a"
  [2]=>
  string(2) "é"
  [3]=>
  string(0) ""
}
array(5) {
  [0]=>
  string(0) ""
  [1]=>
  string(1) "a"
  [2]=>
  string(0) ""
  [3]=>
  string(1) "b"
  [4]=>
  string(0) ""
}
"####, "stderr: {}", out.stderr);
}

/// A dynamic enum name resolves case-insensitively; `Month YYYY` is the first of that month; long keyword chains do not crash.
#[test]
fn test_enum_exists_name_is_case_insensitive_and_strtotime_month_year() {
    let out = compile_and_run_capture_with_regex(r####"<?php
enum Suit { case A; }
$n = 'suit'; var_dump(enum_exists($n));
$n = 'SUIT'; var_dump(enum_exists($n));
$n = '\Suit'; var_dump(enum_exists($n));
$n = 'nope'; var_dump(enum_exists($n));
date_default_timezone_set('UTC');
foreach (['april 2024', 'March 2025', 'Jan 2026', 'april 45', 'april 5 2024', 'april 2024 10:00', 'april 123', 'april 31'] as $s) {
  $t = strtotime($s, 1700000000); var_dump($t === false ? false : date('Y-m-d H:i:s', $t));
}
$r = strtotime(str_repeat("now ", 3000) . "now", 1700000000); var_dump($r === false || is_int($r));
var_dump(strtotime('now now now', 1700000000));
"####);
    assert_eq!(out.stdout, r####"bool(true)
bool(true)
bool(true)
bool(false)
string(19) "2024-04-01 00:00:00"
string(19) "2025-03-01 00:00:00"
string(19) "2026-01-01 00:00:00"
bool(false)
string(19) "2024-04-05 00:00:00"
string(19) "2024-04-01 10:00:00"
bool(false)
string(19) "2023-05-01 00:00:00"
bool(true)
int(1700000000)
"####, "stderr: {}", out.stderr);
}

/// A dynamic class_implements/class_uses name is lowercased once and probed in every relation table.
#[test]
fn test_class_relations_dynamic_name_resolves_interfaces_and_traits() {
    let out = compile_and_run_capture(r####"<?php
interface Shape {}
trait Paints {}
class Box implements Shape { use Paints; }
$names = ['SHAPE', 'shape', 'Paints', 'BOX', 'missing'];
foreach ($names as $n) {
    $r = @class_implements($n);
    echo $n, ': ', $r === false ? 'false' : implode(',', $r), "\n";
}
echo implode(',', class_uses('Box')), "\n";
"####);
    assert_eq!(out.stdout, r####"SHAPE: 
shape: 
Paints: 
BOX: Shape
missing: false
Paints
"####, "stderr: {}", out.stderr);
}

/// Bracket delimiters report php's "No ending matching delimiter" wording and a NUL modifier its own.
#[test]
fn test_preg_pattern_errors_use_phps_bracket_and_nul_wording() {
    let out = compile_and_run_capture_with_regex(
        r####"<?php
preg_match('(abc', 'x');
preg_match('{abc', 'x');
preg_match('[abc', 'x');
preg_match('<abc', 'x');
preg_match('/abc', 'x');
preg_match("/a/\0", 'x');
preg_match('/a/Q', 'x');
"####,
    );
    let mut rest = out.stderr.as_str();
    for expected in [
        "preg_match(): No ending matching delimiter ')' found",
        "preg_match(): No ending matching delimiter '}' found",
        "preg_match(): No ending matching delimiter ']' found",
        "preg_match(): No ending matching delimiter '>' found",
        "preg_match(): No ending delimiter '/' found",
        "preg_match(): NUL byte is not a valid modifier",
        "preg_match(): Unknown modifier 'Q'",
    ] {
        let at = rest
            .find(expected)
            .unwrap_or_else(|| panic!("missing {expected:?} in {}", out.stderr));
        rest = &rest[at + expected.len()..];
    }
}
