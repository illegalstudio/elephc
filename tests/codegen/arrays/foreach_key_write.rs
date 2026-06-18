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