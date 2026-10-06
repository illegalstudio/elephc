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

/// Synthetic nested-append preludes must pass through the ordinary statement checker.
#[test]
fn test_append_review_existing_index_checker() {
    for (source, expected) in [
        ("$items = [[]]; $items[0][] += 5; echo json_encode($items);", "[[5]]"),
        ("$items = [[]]; $items[0][]['k'] = 'v'; echo json_encode($items);", "[[{\"k\":\"v\"}]]"),
        ("class Box { public array $items = [[]]; } $box = new Box(); $box->items[0][] += 3; echo json_encode($box->items);", "[[3]]"),
        ("class Box { public static array $items = [[]]; } Box::$items[0][] += 3; echo json_encode(Box::$items);", "[[3]]"),
    ] {
        let out = compile_and_run_with_heap_debug(&format!("<?php {source}"));
        assert!(out.success, "{source}: {}", out.stderr);
        assert_eq!(out.stdout, expected, "{source}");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{source}: {}", out.stderr);
    }
}

/// Separates constructor and nested-array ownership from the new append expression path.
#[test]
fn test_append_review_nested_property_ownership_control() {
    for source in [
        "class Box { public array $items = [[]]; } $box = new Box(); echo json_encode($box->items);",
        "class Box { public array $items = [[]]; } $box = new Box(); $box->items[0][] = 3; echo json_encode($box->items);",
        "class Box { public static array $items = [[]]; } echo json_encode(Box::$items);",
    ] {
        let out = compile_and_run_with_heap_debug(&format!("<?php {source}"));
        assert!(out.success, "{source}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{source}: {}", out.stderr);
    }
}

/// Sparse literal integer dimensions must not introduce a missing key zero.
#[test]
fn test_append_review_sparse_integer_dimensions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[][1] = 'x';
$items[][1] .= 'y';
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"1\":\"x\"},{\"1\":\"y\"}]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Prefix increments append a fresh one and return the updated value.
#[test]
fn test_append_review_prefix_increment() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
++ $items[];
echo ++$items[], ':', ++$items[]['k'], ':';
class Box { public array $items = []; public static array $shared = []; }
$box = new Box();
++ $box->items[];
++ Box::$shared[];
echo ++$box->items[], ':', ++Box::$shared[], ':';
echo json_encode($items), ':', json_encode($box->items), ':', json_encode(Box::$shared);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:1:1:1:[1,1,{\"k\":1}]:[1,1]:[1,1]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The read and write of a compound append share one float-key conversion diagnostic.
#[test]
fn test_append_review_float_dimension_warns_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$key = 1.5;
$warnings = 0;
set_error_handler(function ($code, $message) use (&$warnings) {
    $warnings++; return true;
});
$items[][$key] .= 'x';
$before = $warnings;
$numbers = [];
echo ++$numbers[][$key], ':';
restore_error_handler();
echo $before, ':', $warnings - $before, ':', $items[0][1];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:2:2:x");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Numeric reads from fresh nested containers yield null rather than a Never operand.
#[test]
fn test_append_review_numeric_missing_leaf() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$value = ++$items[][1];
$compound = [];
$sum = ($compound[][1] += 2);
echo $value, ':', $sum, ':', json_encode($items), ':', json_encode($compound);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:2:[{\"1\":1}]:[{\"1\":2}]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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
