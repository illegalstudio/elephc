//! Purpose:
//! Checks scalar owners overwritten before later frame-wide eval boxing.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - A future eval determines the physical storage used by preceding scalar stores.
//! - Early overwrites and unset must retire those boxed values in both optimizer modes.

use crate::support::*;

/// Scalar stores and unset retire old boxes even when opaque eval occurs later in source order.
#[test]
fn test_future_eval_boxing_releases_earlier_scalar_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$flag = $argc > 0;
$number = 13;
$number = 17;
$fraction = 1.25;
$fraction = 2.5;
echo (int)$flag, ':', $number, ':', $fraction, '|';
unset($flag, $number, $fraction);
$source = $argc > 0 ? 'echo (int)isset($flag), (int)isset($number), (int)isset($fraction);' : '';
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:17:2.5|000", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scalar retirement before eval boxing also holds without EIR optimization.
#[test]
fn test_future_eval_boxing_releases_earlier_scalar_owners_no_ir_opt() {
    without_ir_opt(test_future_eval_boxing_releases_earlier_scalar_owners);
}

/// String keys consumed before later eval widening retire the detached cast copy exactly once.
#[test]
fn test_future_eval_boxing_releases_mixed_array_string_key_copies() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function writeKeysBeforeEval(string $source): void {
    $items = [1, 'a']; $key = str_repeat('k', 40);
    for ($i = 0; $i < 4; $i++) { $items[$key] = $i; }
    eval($source); echo count($items), '|';
    unset($items, $key, $source);
}
function writeKeysWithoutEval(): void {
    $items = [1, 'a']; $key = str_repeat('n', 40);
    for ($i = 0; $i < 4; $i++) { $items[$key] = $i; }
    echo count($items); unset($items, $key);
}
$source = $argc > 0 ? '$seen = strlen($key); unset($seen);' : '';
writeKeysBeforeEval($source); writeKeysWithoutEval(); unset($source);
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3|3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
