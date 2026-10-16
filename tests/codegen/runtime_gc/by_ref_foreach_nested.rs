//! Purpose:
//! Pins nested and re-entered by-reference foreach bindings against PHP output.
//!
//! Called from:
//! - The `codegen_tests` runtime GC test harness.
//!
//! Key details:
//! - Every fixture checks stdout and a clean heap, since a balanced final allocation
//!   count alone does not detect a payload released while an array still references it.
//! - Empty walks preserve the preceding alias; only a successful bind retires raw storage.

use crate::support::{compile_and_run_with_heap_debug, ProgramOutput};

/// Checks PHP-visible output as well as balanced runtime ownership.
fn assert_clean(out: ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Re-entering the inner walk must leave the outer element and last inner alias intact.
#[test]
fn nested_by_ref_foreach_read_only_keeps_entry_payloads_alive() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = [1, 2];
foreach ($a as &$x) {
    foreach ($a as &$y) { echo "$x-$y\n"; }
}
echo implode(',', $a), "\n";
unset($x, $y);
"#), "1-1\n1-2\n2-1\n2-2\n1,2\n");
}

/// Both aliases observe mutations to their shared cell across inner-loop re-entry.
#[test]
fn nested_by_ref_foreach_mutation_keeps_shared_cells_alive() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = [1, 2];
foreach ($a as &$x) {
    foreach ($a as &$y) { $y++; echo "$x-$y\n"; }
}
unset($x, $y);
echo implode(',', $a), "\n";
"#), "2-2\n2-3\n3-3\n4-4\n3,4\n");
}

/// An empty re-entry must preserve an alias even after its source array was replaced.
#[test]
fn nested_by_ref_foreach_empty_reentry_preserves_the_previous_alias() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$rounds = [1, 2];
$a = [1, 2];
foreach ($rounds as $round) {
    foreach ($a as &$y) {}
    if ($round == 1) { $a = []; }
    echo $y, "\n";
}
$y = 9;
echo $y, "\n";
unset($y);
"#), "2\n2\n9\n");
}

/// A raw string assigned after unset is retired before the next successful binding.
#[test]
fn nested_by_ref_foreach_reentry_retires_raw_owned_values() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = ['a', 'b'];
for ($i = 0; $i < 3; $i++) {
    foreach ($a as &$y) { echo $y; }
    unset($y);
    $y = 'held';
}
echo '|', implode(',', $a), '|', $y, "\n";
"#), "ababab|a,b|held\n");
}

/// A by-value parameter keeps its caller's owner while the local becomes an entry alias.
#[test]
fn nested_by_ref_foreach_parameter_rebind_keeps_the_callers_value() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function pairs(array $a, mixed $y): void {
    foreach ($a as &$x) {
        foreach ($a as &$y) { echo "$x-$y\n"; }
    }
    unset($x, $y);
}
$held = ['held'];
pairs([1, 2], $held);
echo implode(',', $held), "\n";
"#), "1-1\n1-2\n2-1\n2-2\nheld\n");
}

/// Zend foreach_005: inner increments are visible to the outer alias at each pair.
#[test]
fn nested_by_ref_foreach_zend_005_mutation() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = [1, 2, 3];
foreach ($a as &$x) {
    foreach ($a as &$y) { echo "$x-$y\n"; $y++; }
}
"#), "1-1\n2-2\n2-3\n3-2\n3-3\n4-4\n5-3\n5-4\n5-5\n");
}

/// Zend foreach_008: deleting current/successor entries preserves both live cursors.
#[test]
fn nested_by_ref_foreach_zend_008_deletion() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = [0, 1, 2, 3];
$walks = 0;
foreach ($a as &$x) {
    foreach ($a as &$y) {
        if (++$walks > 32) { echo "guard\n"; break 2; }
        echo "$x - $y\n";
        if ($x == 0 && $y == 1) { unset($a[2]); unset($a[1]); }
    }
}
"#), "0 - 0\n0 - 1\n0 - 3\n3 - 0\n3 - 3\n");
}

/// Zend foreach_009: a grow with sparse keys relocates both active iterator tables.
#[test]
fn nested_by_ref_foreach_zend_009_resize() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = [0, 1, 2, 3, 4, 5, 6, 7];
unset($a[0], $a[1], $a[2], $a[3]);
$walks = 0;
foreach ($a as &$ref) {
    foreach ($a as &$ref2) {
        if (++$walks > 64) { echo "guard\n"; break 2; }
        echo "$ref-$ref2\n";
        if ($ref == 5 && $ref2 == 6) { $a[42] = 8; }
    }
}
"#), "4-4\n4-5\n4-6\n4-7\n5-4\n5-5\n5-6\n5-7\n5-8\n6-4\n6-5\n6-6\n6-7\n6-8\n7-4\n7-5\n7-6\n7-7\n7-8\n8-4\n8-5\n8-6\n8-7\n8-8\n");
}

/// A fresh empty loop needs no allocated null seed and leaves its value unset.
#[test]
fn nested_by_ref_foreach_fresh_empty_binding_has_no_heap_owner() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = [];
foreach ($a as &$value) {}
var_dump(isset($value));
unset($value);
"#), "bool(false)\n");
}

/// Retiring a raw value on re-entry preserves catchable destructor exceptions.
#[test]
fn nested_by_ref_foreach_raw_reentry_propagates_destructor_throw() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class NestedForeachHeld {
    public function __destruct() { echo "drop\n"; throw new Exception("retired"); }
}
$a = [1];
try {
    for ($i = 0; $i < 2; $i++) {
        foreach ($a as &$y) { echo $y, "\n"; }
        unset($y);
        $y = new NestedForeachHeld();
    }
} catch (Exception $e) { echo $e->getMessage(), "\n"; }
echo "done\n";
"#), "1\ndrop\nretired\ndone\n");
}

/// String-key tombstones retain successor links even after their keys were released.
#[test]
fn nested_by_ref_foreach_deleted_string_successors_reach_the_survivor() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$a = ['zero' => 0, 'one' => 1, 'two' => 2, 'three' => 3];
$walks = 0;
foreach ($a as &$x) {
    foreach ($a as &$y) {
        if (++$walks > 32) { echo "guard\n"; break 2; }
        echo "$x-$y\n";
        if ($x == 0 && $y == 1) { unset($a['two'], $a['one']); }
    }
}
"#), "0-0\n0-1\n0-3\n3-0\n3-3\n");
}
