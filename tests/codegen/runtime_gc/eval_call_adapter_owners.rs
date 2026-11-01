//! Purpose:
//! Balances temporary boxed arguments crossing native-to-eval call adapters.
//!
//! Called from:
//! - Runtime GC integration tests in the remote codegen matrix.
//!
//! Key details:
//! - Native strings and integers create adapter-owned cells that eval only borrows.
//! - Function, static method and named constructor paths share the cleanup contract.

use crate::support::*;

/// Native positional calls retire their boxed arguments after normal and throwing eval calls.
#[test]
fn test_eval_positional_call_adapter_owners_bundle() {
    let source = r#"<?php
$adapterCode = $argc > 0 ? '
function ownedAdapterFunction(string $text, int $number): int { echo $text, $number, "|"; return $number; }
class OwnedAdapterClass {
    public static mixed $calls = 1;
    public function __construct(string $text, int $number) { echo $text, $number, "|"; }
    public static function invoke(string $text, int $number): int { echo $text, $number, "|"; return $number; }
    public static function fail(string $text, int $number): void { throw new RuntimeException($text); }
}' : '';
eval($adapterCode);
echo ownedAdapterFunction('function', 7), '|';
$adapterObject = new OwnedAdapterClass('constructor', 8);
$adapterName = str_repeat('OwnedAdapterClass', 1);
$dynamicAdapter = new $adapterName('dynamic', 11);
$reboundAdapter = new $adapterName(($adapterName = 'stdClass'), 12);
echo OwnedAdapterClass::invoke('static', 9), '|';
echo OwnedAdapterClass::$calls, '|';
try { OwnedAdapterClass::fail('failure', 10); }
catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
unset($adapterObject, $dynamicAdapter, $reboundAdapter, $adapterName, $adapterCode, $error);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "status={} stdout={} stderr={}", out.status, out.stdout, out.stderr);
    assert_eq!(out.stdout, "function7|7|constructor8|dynamic11|stdClass12|static9|9|1|failure|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
