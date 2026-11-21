//! Purpose:
//! End-to-end tests for `preg_replace_callback_array()`, the injected PCRE prelude function.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext preg_replace_callback_array`.
//!
//! Key details:
//! - Expected output is PHP 8.5's.
//! - Eval does not bind this function yet (`EvalImplementationPending`), so only compiled code is
//!   exercised.

use crate::support::*;

/// Entries apply in order, with arrow functions, named functions and closures as callbacks.
#[test]
fn test_preg_replace_callback_array_matches_php() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
function up(array $m): string { return strtoupper($m[0]); }
var_dump(preg_replace_callback_array([
    '/a/' => fn($m) => 'X',
    '/b+/' => 'up',
    '/(\d)/' => function ($m) { return "<" . $m[1] . ">"; },
], 'aabbb1c'));
var_dump(PREG_REPLACE_CALLBACK_ARRAY(['/X/' => fn($m) => 'y', '/y/' => fn($m) => 'z'], 'XaX'));
var_dump(preg_replace_callback_array([], 'same'));
var_dump(@preg_replace_callback_array(['/(/' => fn($m) => 'x'], 'abc'));
"#,
    );
    assert_eq!(
        out.stdout,
        "string(9) \"XXBBB<1>c\"\nstring(3) \"zaz\"\nstring(4) \"same\"\nNULL\n"
    );
}

/// Repeated calls with captured and named callbacks release everything they allocate.
#[test]
fn test_preg_replace_callback_array_heap_clean() {
    let out = compile_and_run_with_heap_debug_and_regex(
        r#"<?php
function up(array $m): string { return strtoupper($m[0]); }
$suffix = "!";
$n = 0;
for ($i = 0; $i < 50; $i++) {
    $r = preg_replace_callback_array(['/a+/' => fn($m) => $m[0] . $suffix, '/b/' => 'up'], 'aab ab');
    $n += strlen($r);
}
echo $r, ":", $n, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "aa!B a!B:400\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A pattern map that arrives through an `array` parameter, as framework code passes it.
#[test]
fn test_preg_replace_callback_array_from_array_parameter() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
function apply(array $map, string $s): ?string { return preg_replace_callback_array($map, $s); }
var_dump(apply(['/a/' => fn($m) => 'X', '/X+/' => fn($m) => strlen($m[0])], 'aab'));
"#,
    );
    assert_eq!(out.stdout, "string(2) \"2b\"\n");
}
