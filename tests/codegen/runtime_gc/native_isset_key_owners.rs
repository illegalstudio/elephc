//! Purpose:
//! Checks key-expression ownership in native array and hash isset probes.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - The consumer history reuses a generated string key in a later empty foreach.
//! - Both optimizer modes must release detached key copies without changing bindings.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Reusing a generated key in a later foreach leaves no storage after each history lifetime.
#[test]
fn test_native_isset_key_owners_reused_foreach_lifetime() {
    let source = include_str!("../../fixtures/foreach_key_lifetime.php");
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "net=0\n".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The same key lifetime is balanced when EIR optimization is disabled.
#[test]
fn test_native_isset_key_owners_reused_foreach_lifetime_no_ir_opt() {
    without_ir_opt(test_native_isset_key_owners_reused_foreach_lifetime);
}

/// Present and missing probes retire fresh keys while concrete borrowed locals stay usable.
#[test]
fn test_native_isset_key_owners_probe_shapes() {
    let check = || {
        let out = compile_and_run_with_heap_debug(r#"<?php
function nativeProbeKey(string $stem): string { return $stem . '0'; }
class NativeKeyProbeBag { public array<string, int> $items = ['present0' => 7]; }
function probeNullableKey(?NativeKeyProbeBag $bag): bool {
    return isset($bag->items[nativeProbeKey('present')]);
}
function inspectNativeKeys(): void {
    array<string, int> $hash = ['present0' => 7];
    array<int> $indexed = [7];
    $key = str_repeat('present0', 1);
    echo (int)isset($hash[$key]), (int)isset($hash[nativeProbeKey('absent')]), ':';
    echo (int)isset($hash[nativeProbeKey('present')]), ':';
    echo (int)isset($indexed[nativeProbeKey('')]), (int)isset($indexed[nativeProbeKey('absent')]), ':';
    echo $indexed[nativeProbeKey('')], ':', ($indexed[nativeProbeKey('missing')] ?? 0), ':';
    echo $key, '|';
    echo (int)probeNullableKey(null), (int)probeNullableKey(new NativeKeyProbeBag());
}
inspectNativeKeys();
"#);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "10:1:10:7:0:present0|01", "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    };
    check();
    without_ir_opt(check);
}

/// Repeated steps exercise the existing-entry read as well as the missing-entry probe.
#[test]
fn test_native_isset_key_owners_review_populated_history() {
    let source = include_str!("../../fixtures/foreach_key_lifetime.php")
        .replace("    $history->step();", "    $history->step();\n    $history->step();");
    assert_eq!(source.matches("$history->step();").count(), 2);
    let check = || {
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "net=0\n".repeat(3), "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    };
    check();
    without_ir_opt(check);
}

/// Native probes borrow live global keys, including boxed integer keys for indexed arrays.
#[test]
fn test_native_isset_key_owners_review_global_keys() {
    let check = || {
        let out = compile_and_run_with_heap_debug(r#"<?php
function probeGlobalKeys(): void {
    global $globalHashKey, $globalArrayKey;
    array<string, int> $hash = ['a' => 1];
    array<int> $indexed = [7];
    echo (int)isset($hash[$globalHashKey]), (int)isset($hash[$globalHashKey]), ':';
    echo (int)isset($indexed[$globalArrayKey]), (int)isset($indexed[$globalArrayKey]), '|';
}
$globalHashKey = str_repeat('a', 1);
$globalArrayKey = $argc - 1;
probeGlobalKeys();
echo $globalHashKey, ':', $globalArrayKey;
"#);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "11:11|a:0", "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    };
    check();
    without_ir_opt(check);
}

/// Ordinary and silent indexed reads retire late-widened keys; renamed bindings stay balanced.
#[test]
fn test_native_isset_key_owners_review_indexed_reads() {
    let source = include_str!("../../fixtures/indexed_key_lifetime.php");
    assert_eq!(source.matches("as $key => $value").count(), 2);
    let control = source.replace("as $key => $value", "as $previousKey => $value");
    assert!(!control.contains("as $key => $value"));
    for source in [source, control.as_str()] {
        let check = || {
            let out = compile_and_run_with_heap_debug(source);
            assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
            let expected = format!("10:10:10\n{}", "local=0 silent=0 call=0\n".repeat(3));
            assert_eq!(out.stdout, expected, "{}", out.stderr);
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        };
        check();
        without_ir_opt(check);
    }
}
