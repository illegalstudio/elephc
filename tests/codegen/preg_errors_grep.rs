//! Purpose:
//! End-to-end tests for `preg_grep()`, `preg_last_error()`, `preg_last_error_msg()` and
//! the `PREG_*` / `PCRE_VERSION*` constants.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through `tests/codegen/mod.rs`.
//!
//! Key details:
//! - Expected output is PHP 8.5's, except `PCRE_VERSION*`, which report the managed
//!   PCRE2 package (10.47) rather than the PHP build's library.
//! - Malformed UTF-8 under `/u` is the portable way to provoke a match error.
//! - The eval fixture builds its source at runtime so it runs through Magician.

use crate::support::*;

/// The error and grep constants have PHP's values; the version constants name PCRE2 10.47.
#[test]
fn test_preg_error_constants() {
    let out = compile_and_run(r#"<?php
echo PREG_NO_ERROR, PREG_INTERNAL_ERROR, PREG_BACKTRACK_LIMIT_ERROR, PREG_RECURSION_LIMIT_ERROR,
    PREG_BAD_UTF8_ERROR, PREG_BAD_UTF8_OFFSET_ERROR, PREG_JIT_STACKLIMIT_ERROR, PREG_GREP_INVERT, "\n";
echo PCRE_VERSION, '|', PCRE_VERSION_MAJOR, '.', PCRE_VERSION_MINOR, "\n";
"#);
    assert_eq!(out, "01234561\n10.47 2025-10-21|10.47\n");
}

/// Every call resets the error state; invalid patterns and malformed UTF-8 set it.
#[test]
fn test_preg_last_error_tracks_each_call() {
    let out = compile_and_run(r#"<?php
function report(): void { echo preg_last_error(), ':', preg_last_error_msg(), "\n"; }
report();
var_dump(@preg_match('/(/', 'a'));
report();
var_dump(preg_match('/a/', 'a'));
report();
var_dump(preg_match('/./u', "\xff"));
report();
var_dump(preg_match('/./u', "\xff", $m), $m);
var_dump(preg_match_all('/./u', "a\xff"));
report();
var_dump(preg_split('/,/u', "a,\xff"));
report();
var_dump(preg_replace('/a/u', 'b', "a\xff"));
report();
var_dump(preg_replace_callback('/a/u', fn($m) => 'b', "a\xff"));
report();
var_dump(preg_replace('/a/', 'b', 'a'));
report();
var_dump(@preg_split('/(/', 'a'));
report();
"#);
    assert_eq!(
        out,
        "0:No error\n\
         bool(false)\n1:Internal error\n\
         int(1)\n0:No error\n\
         bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         bool(false)\narray(0) {\n}\n\
         bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         NULL\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         NULL\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         string(1) \"b\"\n0:No error\n\
         bool(false)\n1:Internal error\n"
    );
}

/// `preg_grep()` keeps keys and original values, honours `PREG_GREP_INVERT`, and fails like PHP.
#[test]
fn test_preg_grep() {
    let out = compile_and_run(r#"<?php
var_dump(preg_grep('/^\d+$/', [1, '2', 3.5, true, 'x', 'k' => '42']));
var_dump(PREG_GREP('/a/', ['a', 'b', 'ca'], PREG_GREP_INVERT));
var_dump(preg_grep(pattern: '/b/', array: ['x' => 'abc', 'y' => 'z']));
var_dump(@preg_grep('/(/', []));
echo preg_last_error(), "\n";
var_dump(preg_grep('/a/u', ['x' => 'a', 5 => "\xff", 7 => 'a']));
echo preg_last_error(), "\n";
"#);
    assert_eq!(
        out,
        "array(4) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"2\"\n  [3]=>\n  bool(true)\n  [\"k\"]=>\n  string(2) \"42\"\n}\n\
         array(1) {\n  [1]=>\n  string(1) \"b\"\n}\n\
         array(1) {\n  [\"x\"]=>\n  string(3) \"abc\"\n}\n\
         bool(false)\n1\n\
         array(1) {\n  [\"x\"]=>\n  string(1) \"a\"\n}\n4\n"
    );
}

/// Eval keeps its own error state with the same codes, messages and `preg_grep()` result.
#[test]
fn test_preg_errors_grep_eval_parity() {
    let out = compile_and_run_with_regex(r#"<?php
$source = 'var_dump(preg_match("/./u", "\xff"));
echo preg_last_error(), ":", preg_last_error_msg(), "|";
preg_match("/a/", "a");
echo preg_last_error(), ":", preg_last_error_msg(), "|";
echo json_encode(preg_grep("/a/", ["k" => "a", "b", "ca"])), json_encode(preg_grep("/a/", ["a", "b"], PREG_GREP_INVERT)), "|";
echo PREG_BAD_UTF8_ERROR, PCRE_VERSION_MAJOR;' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(
        out,
        "bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded|0:No error|{\"k\":\"a\",\"1\":\"ca\"}{\"1\":\"b\"}|410"
    );
}
