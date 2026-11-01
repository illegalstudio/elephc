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

/// Releases promoted hash owners when an indexed loop contract no longer matches concrete storage.
#[test]
fn test_foreach_string_key_loop_contract_ownership() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function rebuild_owned(mixed $source): void {
    $destination = [];
    foreach ($source as $key => $value) {
        $destination[(string)$key] = $value;
    }
    echo count($destination), ";";
}
rebuild_owned(["alpha" => "one", "beta" => "two"]);
rebuild_owned([]);
rebuild_owned(["gamma" => "three"]);
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "2;0;1;");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

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

/// Verifies that `foreach` by value over an indexed array does not visit
/// elements appended during iteration. PHP snapshots the array length at
/// loop entry, so the appended element is not visited (issue #381).
#[test]
fn test_foreach_does_not_visit_appended_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
foreach ($a as $v) {
    echo $v;
    if ($v === 1) $a[] = 3;
}
echo '|' . count($a);
"#,
    );
    assert_eq!(out, "12|3");
}

/// Verifies that `foreach` by reference over an indexed array visits elements
/// appended during iteration. PHP keeps by-reference iteration live against the
/// current array length, unlike by-value iteration (issue #381 guard).
#[test]
fn test_foreach_by_reference_visits_appended_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
foreach ($a as &$v) {
    echo $v;
    if ($v === 1) $a[] = 3;
}
echo '|' . count($a);
"#,
    );
    assert_eq!(out, "123|3");
}

/// Verifies that `foreach` by value over a heterogeneous indexed array also
/// snapshots the length through the dynamic indexed-array iterator path, so
/// appended elements are not visited (issue #381).
#[test]
fn test_foreach_mixed_array_does_not_visit_appended_elements() {
    let out = compile_and_run(
        r#"<?php
$items = [1, "2"];
foreach ($items as $v) {
    echo $v;
    if ($v === 1) $items[] = 3;
}
echo '|';
foreach ($items as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "12|123");
}

/// Verifies a runtime-keyed write into a loop-widened `array<mixed>` releases the reference the
/// slot held before the helper's result replaced it.
///
/// Regression: the helper returns the receiver retained (or a promoted hash) for the storeback,
/// but the raw store left the slot's previous reference owned by nobody. Every write then saw a
/// fake alias, took the copy-on-write clone path and leaked the old array with every value in
/// it: these 20000 writes of a 600-byte string exhausted the 8 MB heap with zero frees.
#[test]
fn test_runtime_key_write_into_widened_array_does_not_leak_the_previous_array() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$b = [];
for ($round = 0; $round < 20000; $round++) {
    $slot = ($round * 31) % 97;
    if ($argc > 9) {
        $b[$slot] = str_repeat('y', 600);
    } else {
        $a[$slot] = str_repeat('x', 600);
    }
}
echo count($a) + count($b), "|", strlen($a[5]);
"#,
    );
    assert_eq!(out, "97|600");
}

/// Verifies a runtime-keyed write consumes the slot's reference whatever the helper returns.
///
/// Regression: the write borrowed the array and retained its result, which balanced only when
/// the pointer came back unchanged. A write that SPLIT a shared array dropped the other owner's
/// reference a second time, so `$b = $a; $a[$k] = ...;` emptied `$b` (and the heap debugger
/// reported free-list corruption); a write that GREW the array released freed storage and left
/// the grown array one reference too many. Reference-bound, static and global destinations take
/// the same path.
#[test]
fn test_runtime_key_write_consumes_the_slot_reference_on_split_and_growth() {
    let out = compile_and_run(
        r#"<?php
function keyOf($i) { return $GLOBALS['argc'] > 50 ? 'k' . $i : $i; }

// A shared array split by the first runtime-keyed write.
function split(): string {
    $a = [];
    for ($i = 0; $i < 3; $i++) { $a[keyOf($i)] = 'a' . $i; }
    $b = $a;
    $a[keyOf(1)] = 'changed';
    $a[keyOf(7)] = 'grown';
    return implode(',', $a) . '|' . implode(',', $b);
}
echo split(), "\n";

// Growth well past the initial capacity, then a string key promoting to a hash.
function grow(): string {
    $a = [];
    $k = 0;
    while ($k < 40) { $a[$k] = str_repeat('x', $k % 3 + 1); $k = $k + 1; }
    $key = $GLOBALS['argc'] > 50 ? 5 : 'name';
    $a[$key] = 'promoted';
    return count($a) . ':' . $a[39] . ':' . $a['name'];
}
echo grow(), "\n";

// A reference-bound local, a static local, and a global written through a runtime key.
function viaRef(): string {
    $a = [];
    $r = &$a;
    for ($i = 0; $i < 12; $i++) { $a[keyOf($i)] = $i * 2; }
    return count($r) . ':' . $r[11];
}
echo viaRef(), "\n";
function viaStatic(): int {
    static $s = [];
    $k = count($s);
    $s[keyOf($k)] = $k;
    return count($s);
}
for ($i = 0; $i < 20; $i++) { viaStatic(); }
echo viaStatic(), "\n";
$g = [];
function viaGlobal(): int {
    global $g;
    for ($i = 0; $i < 15; $i++) { $g[keyOf($i)] = 'g' . $i; }
    return count($g);
}
echo viaGlobal(), ':', $g[14], "\n";

// A loop-widened key copying elements from another mixed array.
function copyAll(array $values): int {
    $chunk = [];
    $k = 0;
    while ($k < 30) { $chunk[$k] = $values[$k]; $k = $k + 1; }
    return count($chunk);
}
$values = [];
for ($i = 0; $i < 30; $i++) { $values[] = $i % 2 === 0 ? 's' . $i : $i; }
$total = 0;
for ($round = 0; $round < 200; $round++) { $total += copyAll($values); }
echo $total, "\n";
"#,
    );
    assert_eq!(out, "a0,changed,a2,grown|a0,a1,a2\n41:x:promoted\n12:22\n21\n15:g14\n6000\n");
}

/// Verifies copying elements between mixed arrays under a loop-widened key does not leak.
///
/// Regression: each copy leaked the boxed element it had read -- the store retains it for the
/// slot and nothing dropped the temporary's own reference -- and every array past its first
/// growth leaked whole. These 400000 copies exhausted the 8 MB heap.
#[test]
fn test_runtime_key_element_copy_in_a_loop_does_not_leak() {
    let out = compile_and_run(
        r#"<?php
function copyAll(array $values): int {
    $chunk = [];
    $k = 0;
    while ($k < 100) {
        $chunk[$k] = $values[$k];
        $k = $k + 1;
    }
    return count($chunk);
}
$values = [];
for ($i = 0; $i < 100; $i++) {
    $values[] = $i % 2 === 0 ? 's' . $i : $i;
}
$total = 0;
for ($round = 0; $round < 4000; $round++) {
    $total += copyAll($values);
}
echo $total, "\n";
"#,
    );
    assert_eq!(out, "400000\n");
}
