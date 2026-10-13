//! Purpose:
//! Regressions for sparse integer writes through boxed PHP-array cells.
//!
//! Called from:
//! - `cargo test --test codegen_tests mixed_sparse_writes`.
//!
//! Key details:
//! - Sparse writes and nested autovivification preserve keys without phantom null entries.
//! - Dense appends remain indexed; promotion preserves COW and balanced payload ownership.

use crate::support::*;

/// Preserves sparse and large integer keys without materializing phantom gap entries.
#[test]
fn mixed_sparse_writes_preserve_keys_and_absent_gap_entries() {
    let out = compile_and_run(
        r#"<?php
function boxed(mixed $value): mixed { return $value; }
$a = boxed([]);
$a[3] = "three";
$a[5] = "five";
$a[3] = "updated";
echo json_encode($a), "|", count($a), "|";
echo array_key_exists(0, $a) ? "phantom" : "absent";
echo "|";
foreach ($a as $key => $value) { echo $key, "=", $value, ";"; }
$a[2147483648] = "large";
echo "|", count($a), "|", $a[2147483648];
"#,
    );
    assert_eq!(out, r#"{"3":"updated","5":"five"}|2|absent|3=updated;5=five;|3|large"#);
}

/// Keeps dense overwrite and append results indexed through boxed PHP-array access.
#[test]
fn mixed_sparse_writes_preserve_dense_overwrite_and_append_fast_paths() {
    let out = compile_and_run(
        r#"<?php
function boxed(mixed $value): mixed { return $value; }
$a = boxed([1, 2]);
$a[0] = 9;
$a[2] = 3;
$a[] = 4;
echo json_encode($a), "|", count($a);
"#,
    );
    assert_eq!(out, "[9,2,3,4]|4");
}

/// Separates a promoted sparse destination without changing its original dense owner.
#[test]
fn mixed_sparse_writes_promotion_preserves_copy_on_write_source() {
    let out = compile_and_run(
        r#"<?php
function boxed(mixed $value): mixed { return $value; }
$original = ["first", "second"];
$changed = boxed($original);
$changed[4] = "far";
$changed[0] = "replacement";
echo json_encode($original), "|", json_encode($changed);
"#,
    );
    assert_eq!(out, r#"["first","second"]|{"0":"replacement","1":"second","4":"far"}"#);
}

/// Preserves keys independently at each autovivified nested array level.
#[test]
fn mixed_sparse_writes_nested_autovivification_preserves_each_level_keys() {
    let out = compile_and_run(
        r#"<?php
function boxed(mixed $value): mixed { return $value; }
$a = boxed([]);
$a[4][7] = "leaf";
$a[4][9] = "other";
$a[6]["name"] = "next";
echo json_encode($a), "|", count($a), "|";
echo array_key_exists(0, $a) ? "outer-phantom" : "outer-absent";
echo "|";
echo array_key_exists(0, $a[4]) ? "inner-phantom" : "inner-absent";
"#,
    );
    assert_eq!(out, r#"{"4":{"7":"leaf","9":"other"},"6":{"name":"next"}}|2|outer-absent|inner-absent"#);
}

/// Publishes promoted nested storage back through the caller's reference parameter.
#[test]
fn mixed_sparse_writes_through_reference_parameter_publish_promoted_payload() {
    let out = compile_and_run(
        r#"<?php
function boxed(mixed $value): mixed { return $value; }
function write_sparse(mixed &$a): void { $a[5] = "value"; $a[7][9] = "nested"; }
$a = boxed([1]);
write_sparse($a);
echo json_encode($a), "|", count($a);
"#,
    );
    assert_eq!(out, r#"{"0":1,"5":"value","7":{"9":"nested"}}|3"#);
}

/// Checks repeated sparse promotion, COW and nested payload cleanup under heap debugging.
#[test]
fn mixed_sparse_writes_repeated_promotion_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function boxed(mixed $value): mixed { return $value; }
function exercise_sparse(): void {
    $source = [str_repeat("a", 32), str_repeat("b", 32)];
    $a = boxed($source);
    $a[5] = str_repeat("c", 32);
    $a[8][13] = str_repeat("d", 32);
    $a[0] = str_repeat("e", 32);
    if (count($source) !== 2 || count($a) !== 4 || array_key_exists(1, $a[8])) {
        throw new RuntimeException("sparse shape or COW corrupted");
    }
    unset($source);
    unset($a);
}
for ($i = 0; $i < 3; $i++) { exercise_sparse(); }
echo "sparse-clean";
"#,
    );
    assert_eq!(out.stdout, "sparse-clean", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Retires promoted nested payloads when an exception abandons their owning frame.
#[test]
fn mixed_sparse_writes_exceptional_promotion_cleanup_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function boxed(mixed $value): mixed { return $value; }
function fail_after_sparse_write(): void {
    $a = boxed([str_repeat("s", 32)]);
    $a[4][8] = str_repeat("payload", 8);
    throw new RuntimeException("after-promotion");
}
try { fail_after_sparse_write(); }
catch (RuntimeException $error) { echo $error->getMessage(); unset($error); }
"#,
    );
    assert_eq!(out.stdout, "after-promotion", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
