//! Purpose:
//! Covers callable property types refined by method-body checking after the initial main pass.
//!
//! Called from:
//! - The native callable codegen suite.
//!
//! Key details:
//! - Both property reads and copied locals must use the final metadata when binding parameters.

use crate::support::{compile_and_run_tagged, compile_and_run_with_heap_debug};

/// Method writes settle instance/static callable metadata before direct and copied arguments are checked.
#[test]
fn test_callable_property_method_writes_settle_parameter_binding() {
    let source = r#"<?php
class MethodCallbackStore {
    public $callback;
    public static $shared;
    public function install(int $number): void {
        $this->callback = static fn(): int => $number;
    }
    public static function installShared(int $number): void {
        self::$shared = static fn(): int => $number;
    }
    public function invoke(callable $callback): int { return $callback(); }
    public static function invokeShared(callable $callback): int { return $callback(); }
}
$store = new MethodCallbackStore();
$store->install(7);
echo $store->invoke($store->callback), "|";
$copy = $store->callback;
echo $store->invoke($copy), "|";
MethodCallbackStore::installShared(9);
echo MethodCallbackStore::invokeShared(MethodCallbackStore::$shared), "|";
$shared = MethodCallbackStore::$shared;
echo MethodCallbackStore::invokeShared($shared), "|";
unset($store, $copy, $shared);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7|7|9|9|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "7|7|9|9|");
}
