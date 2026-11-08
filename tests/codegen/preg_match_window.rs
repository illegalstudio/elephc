//! Purpose:
//! End-to-end tests for the `$flags` / `$offset` parameters of `preg_match()` and the
//! `$offset` parameter of `preg_match_all()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through `tests/codegen/mod.rs`.
//!
//! Key details:
//! - Expected output is PHP 8.5's; `ELEPHC_PHP_CHECK=1` re-runs each fixture under `php`.
//! - Offsets keep the whole subject visible, so anchors and lookbehind are exercised.
//! - The eval fixture builds its source at runtime so it runs through Magician.

use crate::support::*;

/// `PREG_OFFSET_CAPTURE` and `PREG_UNMATCHED_AS_NULL` shape `preg_match()` captures like PHP.
#[test]
fn test_preg_match_capture_flags() {
    let out = compile_and_run(r#"<?php
echo preg_match('/(b)(x)?(c)?/', 'abcd', $m, PREG_OFFSET_CAPTURE), json_encode($m), "\n";
echo preg_match('/(b)(x)?(z)?/', 'abcd', $m, PREG_UNMATCHED_AS_NULL), json_encode($m), "\n";
echo preg_match('/(b)(x)?/', 'abcd', $m, PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE), json_encode($m), "\n";
echo preg_match('/(b)(x)?(z)?/', 'abcd', $m, 0), json_encode($m), "\n";
echo preg_match('/q/', 'abcd', $m, PREG_OFFSET_CAPTURE), json_encode($m), "\n";
"#);
    assert_eq!(
        out,
        "1[[\"bc\",1],[\"b\",1],[\"\",-1],[\"c\",2]]\n\
         1[\"b\",\"b\",null,null]\n\
         1[[\"b\",1],[\"b\",1],[null,-1]]\n\
         1[\"b\",\"b\"]\n\
         0[]\n"
    );
}

/// `$offset` starts the search without hiding the subject from anchors and lookbehind.
#[test]
fn test_preg_match_offset() {
    let out = compile_and_run(r#"<?php
echo preg_match('/a/', 'abca', $m, PREG_OFFSET_CAPTURE, 1), json_encode($m), "\n";
echo preg_match('/a/', 'abca', $m, PREG_OFFSET_CAPTURE, -1), json_encode($m), "\n";
echo preg_match('/a/', 'abca', $m, PREG_OFFSET_CAPTURE, -10), json_encode($m), "\n";
echo preg_match('/^b/', 'abc', $m, 0, 1), preg_match('/(?<=a)b/', 'abc', $m, 0, 1), "\n";
echo preg_match('/\Gb/', 'abc', $m, 0, 1), preg_match('/c$/', 'abc', $m, 0, 3), "\n";
echo \PrEg_MaTcH('/a/', 'xa', offset: 1), preg_match(subject: 'xa', pattern: '/a/', offset: 2), "\n";
"#);
    assert_eq!(out, "1[[\"a\",3]]\n1[[\"a\",3]]\n1[[\"a\",0]]\n01\n10\n10\n");
}

/// An offset past the end returns false and still writes the empty `$matches` array.
#[test]
fn test_preg_match_offset_past_end() {
    let out = compile_and_run(r#"<?php
$m = 'keep';
var_dump(preg_match('/a/', 'abc', $m, 0, 4));
echo json_encode($m), "\n";
$all = 'keep';
var_dump(preg_match_all('/a/', 'abc', $all, 0, 4));
echo json_encode($all), "\n";
var_dump(preg_match('/a/', 'abc', $m, 0, 3));
"#);
    assert_eq!(out, "bool(false)\n[]\nbool(false)\n[]\nint(0)\n");
}

/// Order flags are rejected for `preg_match()` with PHP's `ValueError`; other bits are ignored.
#[test]
fn test_preg_match_invalid_flags() {
    let out = compile_and_run(r#"<?php
foreach ([PREG_PATTERN_ORDER, PREG_SET_ORDER, 255] as $flag) {
    try {
        preg_match('/a/', 'a', $m, $flag);
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
echo preg_match('/a/', 'a', $m, 1024), json_encode($m), "\n";
"#);
    assert_eq!(
        out,
        "preg_match(): Argument #4 ($flags) must be a PREG_* constant\n\
         preg_match(): Argument #4 ($flags) must be a PREG_* constant\n\
         preg_match(): Argument #4 ($flags) must be a PREG_* constant\n\
         1[\"a\"]\n"
    );
}

/// `preg_match_all()` honours `$offset` with and without a `$matches` destination.
#[test]
fn test_preg_match_all_offset() {
    let out = compile_and_run(r#"<?php
echo preg_match_all('/a/', 'abab', $m, PREG_OFFSET_CAPTURE, 1), json_encode($m), "\n";
echo preg_match_all('/a/', 'abab', $m, PREG_PATTERN_ORDER, -2), json_encode($m), "\n";
echo preg_match_all('/\Ga/', 'aab', $m, 0, 1), json_encode($m), "\n";
echo preg_match_all('/a/', 'abab', offset: 1), preg_match_all('/a/', 'abab', offset: -10), "\n";
"#);
    assert_eq!(
        out,
        "1[[[\"a\",2]]]\n1[[\"a\"]]\n1[[\"a\"]]\n12\n"
    );
}

/// `PREG_SET_ORDER` rows drop trailing unmatched groups unless they become nulls.
#[test]
fn test_preg_match_all_set_order_trims_trailing_groups() {
    let out = compile_and_run(r#"<?php
preg_match_all('/(a)(x)?/', 'baba', $m, PREG_SET_ORDER);
echo json_encode($m), "\n";
preg_match_all('/(a)(x)?/', 'baba', $m, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL);
echo json_encode($m), "\n";
preg_match_all('/(x)?(a)/', 'ba', $m, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
echo json_encode($m), "\n";
"#);
    assert_eq!(
        out,
        "[[\"a\",\"a\"],[\"a\",\"a\"]]\n\
         [[\"a\",\"a\",null],[\"a\",\"a\",null]]\n\
         [[[\"a\",1],[\"\",-1],[\"a\",1]]]\n"
    );
}

/// Eval runs the same `$flags` / `$offset` semantics through Magician.
#[test]
fn test_preg_match_window_eval_parity() {
    let out = compile_and_run_with_regex(r#"<?php
$source = 'echo preg_match("/(b)(x)?/", "abcb", $m, PREG_OFFSET_CAPTURE | PREG_UNMATCHED_AS_NULL, 2), json_encode($m), "|";
echo preg_match_all("/(a)(x)?/", "baba", $all, PREG_SET_ORDER, 2), json_encode($all), "|";
var_dump(preg_match("/a/", "a", $m, 0, 5));
try { preg_match("/a/", "a", $m, PREG_SET_ORDER); } catch (ValueError $e) { echo $e->getMessage(); }' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(
        out,
        "1[[\"b\",3],[\"b\",3],[null,-1]]|1[[\"a\",\"a\"]]|bool(false)\n\
         preg_match(): Argument #4 ($flags) must be a PREG_* constant"
    );
}

/// Boxed `$flags` / `$offset` values are cast to integers, so a capture can drive the next search.
#[test]
fn test_preg_match_boxed_offset_and_flags() {
    let out = compile_and_run(r#"<?php
$subject = 'hello preg world';
$offset = 0;
$options = ['flags' => PREG_OFFSET_CAPTURE, 'name' => 'tokens'];
$flags = $options['flags'];
while (preg_match('/\w+/', $subject, $token, $flags, $offset) === 1) {
    echo $token[0][0], '@', $token[0][1], ' ';
    $offset = $token[0][1] + strlen($token[0][0]);
}
echo preg_match_all('/o/', $subject, $all, $flags, $options['flags'] - 250), json_encode($all), "\n";
"#);
    assert_eq!(out, "hello@0 preg@6 world@11 1[[[\"o\",12]]]\n");
}
