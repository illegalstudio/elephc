//! Purpose:
//! Checks persistent automatic hash indices through typed and Mixed array append.
//!
//! Called from:
//! - Focused codegen integration tests through Rust's test harness.
//!
//! Key details:
//! - Deletion retains integer-key history, including negatives and saturation.
//! - Runtime-unknown branches keep the Mixed receiver path observable.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug};

/// Shares native automatic-key history with dynamically evaluated array mutation.
#[test]
fn test_hash_append_history_in_dynamic_eval() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = '$a = ["seed" => 1, 9 => 2]; unset($a[9]); $a[] = 3; echo $a[10], "\n";
$b = $a; $b[] = 4; echo $b[11], ":", $a[10], "\n";
$dense = [1, 2]; unset($dense[1]); $dense[] = 3; echo $dense[2], "\n";
function append_rhs() { echo "rhs\n"; return 3; }
$full = ["seed" => 1, 9223372036854775807 => 2]; try { $full[] = append_rhs(); } catch (Error $error) { echo $error->getMessage(), "\n"; } echo $full[9223372036854775807], "\n";';
if ($argc > 1) { $source .= " "; }
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3\n4:3\n3\nrhs\nCannot add element to the array as the next element is already occupied\n2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Keeps deleted-key history when a hash crosses the native Zend zval boundary in both directions.
#[test]
fn test_hash_append_history_survives_zval_roundtrip() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$a = ["seed" => 1, -5 => 2];
unset($a[-5]);
$wire = zval_pack($a);
$copy = zval_unpack($wire);
zval_free($wire);
$copy[] = 3;
echo $copy[-4], "\n";
$full = ["seed" => 1, PHP_INT_MAX => 2];
unset($full[PHP_INT_MAX]);
$wire = zval_pack($full);
$copy = zval_unpack($wire);
zval_free($wire);
$copy[] = 4;
echo $copy[PHP_INT_MAX], "\n";
try { $copy[] = 5; }
catch (Error $error) { echo $error->getMessage(), "\n"; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3\n4\nCannot add element to the array as the next element is already occupied\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Preserves deleted positive and negative indices through independent COW appends.
#[test]
fn test_hash_append_preserves_deleted_integer_history() {
    let out = compile_and_run(r#"<?php
$a = ["seed" => 1, 9 => 2];
unset($a[9]);
$b = $a;
$a[] = 3;
$b[] = 4;
foreach ($a as $key => $value) { echo $key, ":", $value, ";"; }
echo "\n";
foreach ($b as $key => $value) { echo $key, ":", $value, ";"; }
echo "\n";
$negative = ["seed" => 1, -5 => 2];
unset($negative[-5]);
$negative[] = 3;
foreach ($negative as $key => $value) { echo $key, ":", $value, ";"; }
echo "\n";
"#);
    assert_eq!(out, "seed:1;10:3;\nseed:1;10:4;\nseed:1;-4:3;\n");
}

/// Reuses an unset maximum integer key once, then throws without corrupting its value.
#[test]
fn test_hash_append_maximum_key_raises_catchable_error() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$a = ["seed" => 1, PHP_INT_MAX => 2];
unset($a[PHP_INT_MAX]);
$a[] = 3;
try { $a[] = 4; }
catch (Error $error) { echo $error->getMessage(), "\n"; }
echo count($a), ":", $a[PHP_INT_MAX], "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Cannot add element to the array as the next element is already occupied\n2:3\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Uses the same persistent counter and exhaustion rule when the receiver is boxed Mixed.
#[test]
fn test_hash_append_mixed_preserves_history_and_exhaustion_ownership() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$base = ["seed" => 1, 9 => 2];
unset($base[9]);
$a = $argc == 1 ? $base : null;
$a[] = 3;
echo $a[10], "\n";
$full = $argc == 1 ? ["seed" => 1, PHP_INT_MAX => 2] : null;
try { $full[] = str_repeat("x", $argc + 2); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
echo count($full), ":", $full[PHP_INT_MAX], "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3\nCannot add element to the array as the next element is already occupied\n2:2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
