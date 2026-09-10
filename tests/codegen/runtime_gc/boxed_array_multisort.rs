//! Purpose:
//! Verifies boxed two-array `array_multisort` ordering, COW publication and validation.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Declared array parameters use boxed cells even when their physical payloads are packed.
//! - Both receivers must detach from value aliases, and exceptional exits must keep owners balanced.

use crate::support::*;

/// Concrete reads from eval-widened slots retire both the old box and every detached lease.
#[test]
fn test_core_array_multisort_post_eval_same_and_distinct_receivers_keep_heap_clean() {
    let source = r#"<?php
function multisortWidenedSlots(string $source): void {
    $same = [];
    $left = [];
    $right = [];
    eval($source);
    $same = [3, 1, 2];
    $sameSnapshot = $same;
    array_multisort($same, $same);
    echo implode(",", $same), ":", implode(",", $sameSnapshot), "|";
    $left = [2, 1, 2];
    $right = [3, 2, 1];
    $leftSnapshot = $left;
    $rightSnapshot = $right;
    array_multisort($left, $right);
    echo implode(",", $left), ":", implode(",", $right), ":";
    echo implode(",", $leftSnapshot), ":", implode(",", $rightSnapshot), "|";
}
$source = 'return null; // ' . $argc;
for ($i = 0; $i < 3; $i++) { multisortWidenedSlots($source); }
unset($source);
"#;
    let expected = "1,2,3:3,1,2|1,2,2:2,1,3:2,1,2:3,2,1|".repeat(3);
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Reference aliases sort once while separate value aliases keep independent COW snapshots.
#[test]
fn test_core_array_multisort_same_reference_and_distinct_value_aliases() {
    let source = r#"<?php
function multisortAliasInput(): array { return [3, "1", 2]; }
function multisortAliasRows(array &$primary, array &$secondary): void {
    array_multisort($primary, $secondary);
}
$direct = [3, 1, 2];
$directSnapshot = $direct;
echo array_multisort($direct, $direct) ? "true|" : "false|";
echo implode(",", $direct), ":", implode(",", $directSnapshot), "|";
$same = multisortAliasInput();
$sameSnapshot = $same;
multisortAliasRows($same, $same);
echo implode(",", $same), ":", implode(",", $sameSnapshot), "|";
$bound = multisortAliasInput();
$boundSnapshot = $bound;
$boundAlias =& $bound;
array_multisort($bound, $boundAlias);
echo implode(",", $bound), ":", implode(",", $boundSnapshot), "|";
unset($boundAlias, $bound, $boundSnapshot);
$left = multisortAliasInput();
$right = $left;
$snapshot = $left;
multisortAliasRows($left, $right);
echo implode(",", $left), ":", implode(",", $right), ":", implode(",", $snapshot), "|";
class MultisortAliasHolder { public array $values = [3, 1, 2]; }
$holder = new MultisortAliasHolder();
$propertySnapshot = $holder->values;
array_multisort($holder->values, $holder->values);
echo implode(",", $holder->values), ":", implode(",", $propertySnapshot);
unset($direct, $directSnapshot, $same, $sameSnapshot, $left, $right, $snapshot, $holder, $propertySnapshot);
"#;
    let expected = "true|1,2,3:3,1,2|1,2,3:3,1,2|1,2,3:3,1,2|1,2,3:1,2,3:3,1,2|1,2,3:3,1,2";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Declared arrays sort by both columns while preserving both pre-call COW snapshots.
#[test]
fn test_core_boxed_array_multisort_orders_pairs_and_preserves_both_aliases() {
    let source = r#"<?php
function sortRows(array &$primary, array &$secondary): bool {
    return array_multisort($primary, $secondary);
}
$primary = [2, 1, 2];
$secondary = [str_repeat("z", 2), str_repeat("m", 2), str_repeat("a", 2)];
$primaryAlias = $primary;
$secondaryAlias = $secondary;
echo sortRows($primary, $secondary) ? "true|" : "false|";
echo implode(",", $primary), ":", implode(",", $secondary), "|";
echo implode(",", $primaryAlias), ":", implode(",", $secondaryAlias), "|";
$words = [str_repeat("b", 2), str_repeat("a", 2), str_repeat("b", 2)];
$weights = [3.5, 2.5, 1.5];
sortRows($words, $weights);
echo implode(",", $words), ":", implode(",", $weights), "|";
$emptyPrimary = [];
$emptySecondary = [];
echo sortRows($emptyPrimary, $emptySecondary) ? "empty:" : "bad:";
echo count($emptyPrimary), ":", count($emptySecondary);
unset(
    $primary,
    $secondary,
    $primaryAlias,
    $secondaryAlias,
    $words,
    $weights,
    $emptyPrimary,
    $emptySecondary
);
"#;
    let expected = "true|1,2,2:mm,aa,zz|2,1,2:zz,mm,aa|aa,bb,bb:2.5,1.5,3.5|empty:0:0";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Unequal declared arrays throw before moving rows and leave both receivers and aliases valid.
#[test]
fn test_core_boxed_array_multisort_unequal_lengths_preserve_owners() {
    let source = r#"<?php
function sortRows(array &$primary, array &$secondary): void {
    array_multisort($primary, $secondary);
}
$primary = [2, 1];
$secondary = [str_repeat("x", 2)];
$primaryAlias = $primary;
$secondaryAlias = $secondary;
try {
    sortRows($primary, $secondary);
    echo "missed|";
} catch (ValueError $error) {
    echo $error->getMessage(), "|";
    unset($error);
}
echo implode(",", $primary), ":", implode(",", $secondary), "|";
echo implode(",", $primaryAlias), ":", implode(",", $secondaryAlias);
unset($primary, $secondary, $primaryAlias, $secondaryAlias);
"#;
    let expected = "array_multisort(): Array sizes are inconsistent|2,1:xx|2,1:xx";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Boxed hash layouts are rejected before COW instead of being read as packed array slots.
#[test]
fn test_core_boxed_array_multisort_rejects_hash_layout_without_reinterpretation() {
    let source = r#"<?php
function sortRows(array &$primary, array &$secondary): void {
    array_multisort($primary, $secondary);
}
$primary = ["left" => 2, "right" => 1];
$secondary = ["first" => "b", "second" => "a"];
$primaryAlias = $primary;
$secondaryAlias = $secondary;
try {
    sortRows($primary, $secondary);
    echo "missed|";
} catch (TypeError $error) {
    echo $error->getMessage(), "|";
    unset($error);
}
echo implode(",", array_keys($primary)), ":", implode(",", $primary), "|";
echo implode(",", array_keys($secondary)), ":", implode(",", $secondary), "|";
echo implode(",", $primaryAlias), ":", implode(",", $secondaryAlias);
unset($primary, $secondary, $primaryAlias, $secondaryAlias);
"#;
    let expected = "array_multisort() arguments must be indexed arrays|left,right:2,1|first,second:b,a|2,1:b,a";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Boxed containers are rejected by the scalar-only comparator instead of comparing cell pointers.
#[test]
fn test_core_boxed_array_multisort_rejects_non_scalar_elements() {
    let error = compile_and_run_expect_failure(
        r#"<?php
function sortRows(array &$primary, array &$secondary): void {
    array_multisort($primary, $secondary);
}
$primary = [[2], [1]];
$secondary = ["b", "a"];
sortRows($primary, $secondary);
"#,
    );
    assert!(
        error.contains("sorting Mixed arrays containing non-scalar values is not supported"),
        "{error}"
    );
}
