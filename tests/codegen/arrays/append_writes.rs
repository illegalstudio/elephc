//! Purpose:
//! Regressions for write-only empty dimensions in assignments and updates.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Append uses the runtime next-index operation rather than a guessed numeric key.
//! - Expression results and receiver evaluation must preserve PHP source order.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Statement and expression compound appends create a fresh element, not an array read.
#[test]
fn test_append_review_compound_and_assignment_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [1];
$items[] += 2;
echo ($items[] = 3), ':', ($items[] += 4), ':';
foreach ($items as $value) { echo $value, ','; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3:4:1,2,3,4,");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested empty dimensions construct distinct containers and preserve expression results.
#[test]
fn test_append_review_nested_dimensions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[]['k'] = 'x';
echo ($items[]['k'] .= 'y'), ':';
$items[][] = 1;
echo $items[0]['k'], ':', $items[1]['k'], ':', $items[2][0];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "y:x:y:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Local, object-property and static-property appends share fresh-null compound semantics.
#[test]
fn test_append_review_property_receivers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box { public array $items = []; public static array $shared = []; }
$box = new Box();
$box->items[] += 2;
Box::$shared[] += 3;
echo $box->items[0], ':', Box::$shared[0];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:3");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Post-increment of a fresh append yields null and stores one.
#[test]
fn test_append_review_post_increment() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[]++;
echo ($items[]++), ':', $items[0], ':', $items[1];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, ":1:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Arrow functions capture the append receiver by value even when it lives in a prelude.
#[test]
fn test_append_review_arrow_capture() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box { public array $items = [1]; }
$box = new Box();
$append = fn() => ($box->items[] += 2);
echo $append(), ':', count($box->items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:2");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime-unknown RHS values have the same append behavior with EIR optimization disabled.
#[test]
fn test_append_review_without_ir_optimization() {
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(r#"<?php
$items = [];
echo ($items[] += $argc), ':', $items[0], ':';
$items[][] = $argc;
echo $items[1][0];
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:1:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A warning handler cannot redirect the write away from the key its compound read diagnosed.
#[test]
fn test_append_review_warning_handler_key_snapshot() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = 'a';
$items = [];
set_error_handler(function ($code, $message) use (&$key) {
    $key = 'b'; echo 'warn:'; return true;
});
$items[][$key] .= 'x';
restore_error_handler();
foreach ($items[0] as $name => $value) { echo $name, ':', $value; }
echo ':', $key;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "warn:a:x:b");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// RHS effects precede append and the runtime retains deleted-key history.
#[test]
fn test_append_review_rhs_order_and_key_history() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[] += ($items[] = 7);
echo $items[0], ':', $items[1], ':';
$assoc = ['seed' => 1, 9 => 2];
unset($assoc[9]);
echo ($assoc[] += 3), ':', $assoc[10];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7:7:3:3");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
