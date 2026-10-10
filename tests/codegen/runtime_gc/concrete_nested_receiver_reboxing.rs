//! Purpose:
//! Verifies concrete local arrays are reboxed after a nested write's RHS replaces its root.
//!
//! Called from:
//! - The runtime GC codegen suite on each executable supported target.
//!
//! Key details:
//! - Literal-inferred locals, not mixed parameters or properties, exercise the reboxing path.
//! - Retained aliases and repeated owned strings expose lost writes and unbalanced cleanup.

use crate::support::*;

/// A reassigned concrete hash root receives the nested write while its old alias survives.
#[test]
fn test_concrete_hash_nested_receiver_reboxing_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function concrete_hash(int $n): string {
    $items = ["a" => ["old" => "before" . $n]];
    $alias = $items;
    $items["a"]["b"] = ($items = ["a" => ["kept" => "after" . $n]])
        ? "written" . $n : "never";
    return json_encode($items) . "|" . json_encode($alias);
}
$out = "";
for ($n = 0; $n < 20 + ($argc > 5 ? 1 : 0); $n++) { $out = concrete_hash($n); }
echo $out;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r#"{"a":{"kept":"after19","b":"written19"}}|{"a":{"old":"before19"}}"#);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An indexed root is reboxed without losing its nested strings or the pre-reassignment alias.
#[test]
fn test_concrete_indexed_nested_receiver_reboxing_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function concrete_indexed(int $n): string {
    $items = [["before" . $n]];
    $alias = $items;
    $items[0][1] = ($items = [["after" . $n]]) ? "written" . $n : "never";
    return json_encode($items) . "|" . json_encode($alias);
}
$out = "";
for ($n = 0; $n < 20 + ($argc > 5 ? 1 : 0); $n++) { $out = concrete_indexed($n); }
echo $out;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r#"[["after19","written19"]]|[["before19"]]"#);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
