//! Purpose:
//! Regression tests for writing into a statically `Array(Mixed)` destination
//! under a boxed Mixed foreach key, where the key tag (integer vs string) is
//! only known at runtime.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - PHP `foreach` keys are always `Mixed` in EIR (`Op::IterCurrentKey`), so a
//!   `foreach($src as $k=>$v) $dst[$k]=$v` rebuild into an `array`-typed
//!   destination must not coerce the key to int (which collapsed string keys
//!   onto index 0 and dropped all but the last entry). The fix routes such
//!   writes through `Op::ArraySetMixedKey`, whose runtime helper keeps integer
//!   keys on indexed storage and promotes string keys to a hash.
//! - Integer-keyed rebuilds must stay on indexed storage so indexed consumers
//!   like `implode` keep working (the no-regression case).
//! - Verification reads the rebuilt array back through `foreach` (runtime
//!   representation-aware) rather than `$dst[$strKey]`, because the destination
//!   stays statically `Array(Mixed)` even when its runtime storage is a hash.

use crate::support::*;

/// Rebuilds a string-keyed `array` source through a foreach key write and
/// verifies every entry survives (previously all but the last collapsed onto
/// index 0).
#[test]
fn test_foreach_mixed_string_key_rebuild() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) {
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild(["alpha" => 1, "beta" => 2, "gamma" => 3]);
foreach ($r as $k => $v) {
    echo $k, "=", $v, ";";
}
"#,
    );
    assert_eq!(out, "alpha=1;beta=2;gamma=3;");
}

/// Rebuilds a string-keyed `array` source and verifies the entry count, since
/// the pre-fix bug collapsed three string keys onto index 0 (count 1).
#[test]
fn test_foreach_mixed_string_key_rebuild_count() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) {
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild(["alpha" => 1, "beta" => 2, "gamma" => 3]);
echo count($r);
"#,
    );
    assert_eq!(out, "3");
}

/// Rebuilds an integer-keyed heterogeneous `array` source through a foreach key
/// write and verifies `implode` still works: integer keys must stay on indexed
/// storage (the no-regression case for indexed consumers).
#[test]
fn test_foreach_mixed_int_key_stays_indexed() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) {
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild([1, "two", 3.5]);
echo implode(",", $r);
"#,
    );
    assert_eq!(out, "1,two,3.5");
}

/// Rebuilds a mixed integer- and string-keyed `array` source, exercising an
/// integer key written after the destination has already been promoted to a
/// hash by an earlier string key (the already-hash + integer-key path).
#[test]
fn test_foreach_mixed_int_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) {
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild(["a" => 1, 0 => 2, "b" => 3]);
foreach ($r as $k => $v) {
    echo $k, "=", $v, ";";
}
"#,
    );
    assert_eq!(out, "a=1;0=2;b=3;");
}

/// Rebuilds a string-keyed `array` source into a destination that already holds
/// an integer-keyed entry, verifying the promotion path composes with a prior
/// indexed write.
#[test]
fn test_foreach_mixed_string_key_after_int_seed() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    $dst[] = 9;
    foreach ($src as $k => $v) {
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild(["x" => 7]);
echo count($r), ";";
foreach ($r as $k => $v) {
    echo $k, "=", $v, ";";
}
"#,
    );
    assert_eq!(out, "2;0=9;x=7;");
}

/// Rebuilds a sparse integer-keyed source (non-contiguous keys) through a foreach
/// key write. A key beyond the current logical end must promote the destination to
/// a hash so the gap is preserved, instead of the indexed path zero-filling every
/// slot between the end and the target index (regression: `[0,5,2]` keys used to
/// read back as `0..5` with empty filler slots).
#[test]
fn test_foreach_mixed_sparse_int_keys() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) {
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild([0 => "a", 5 => "b", 2 => "c"]);
foreach ($r as $k => $v) {
    echo $k, "=", $v, ";";
}
"#,
    );
    assert_eq!(out, "0=a;5=b;2=c;");
}

/// Rebuilds a negative integer-keyed source through a foreach key write. Negative
/// keys cannot live in packed indexed storage, so the destination must promote to
/// a hash (regression: the indexed path silently dropped negative-index writes,
/// leaving only the last non-negative key).
#[test]
fn test_foreach_mixed_negative_int_keys() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) {
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild([-3 => "a", -1 => "b", 0 => "c"]);
foreach ($r as $k => $v) {
    echo $k, "=", $v, ";";
}
"#,
    );
    assert_eq!(out, "-3=a;-1=b;0=c;");
}

/// A `foreach` key name used in one function must not leak its boxed-`Mixed`-key
/// classification into another function that reuses the same variable name as a
/// genuine string key. `build()` (checked first) registers `$k` as a foreach key;
/// before the per-function reset of `foreach_key_locals`, `lookup()` routed
/// `$m[$k]` onto the `Array(Mixed)` path and the direct `$m["name"]` read returned
/// the wrong value.
#[test]
fn test_foreach_key_name_does_not_leak_across_functions() {
    let out = compile_and_run(
        r#"<?php
function build(array $src): array {
    $out = [];
    foreach ($src as $k => $v) {
        $out[$k] = $v;
    }
    return $out;
}
function lookup(): string {
    $m = [];
    $k = "name";
    $m[$k] = "Alice";
    return $m["name"];
}
$b = build([1, 2, 3]);
echo implode(",", $b), ";";
echo lookup();
"#,
    );
    assert_eq!(out, "1,2,3;Alice");
}

/// Reassigning the `foreach` key variable to a string inside the loop makes it an
/// ordinary string key, so the destination must promote to associative storage
/// like PHP. Before the foreach-key marker was dropped on direct reassignment, the
/// checker forced `Array(Mixed)` while the lowering took the string-promotion path,
/// producing a spurious `AssocArray -> Array(Mixed)` backend error.
#[test]
fn test_foreach_key_reassigned_to_string() {
    let out = compile_and_run(
        r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) {
        $k = "fixed";
        $dst[$k] = $v;
    }
    return $dst;
}
$r = rebuild(["a" => 1, "b" => 2]);
foreach ($r as $k => $v) {
    echo $k, "=", $v, ";";
}
"#,
    );
    assert_eq!(out, "fixed=2;");
}