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

/// Runs a static callable-property fixture and asserts heap-clean output plus equivalent tagged behavior.
fn assert_static_callable_output(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Overwriting a static callable property must retire the previous descriptor while copied locals,
/// borrowed-parameter stores and captured-object destructors stay balanced. The final owner lives
/// in the static slot until program exit, where its captured payload destructor runs exactly once.
#[test]
fn test_static_callable_property_overwrite_retires_captured_owners() {
    assert_static_callable_output(
        r#"<?php
class StaticCallablePayload {
    public int $number;
    public function __construct(int $number) { $this->number = $number; }
    public function __destruct() { echo "drop", $this->number, "|"; }
}
class StaticCallableOwner {
    public static $shared;
    public static function install(int $number): void {
        $payload = new StaticCallablePayload($number);
        self::$shared = static fn(): int => $payload->number;
    }
    public static function borrow(callable $callback): void { self::$shared = $callback; }
    public static function invoke(callable $callback): int { return $callback(); }
}
StaticCallableOwner::install(1);
$first = StaticCallableOwner::$shared;
StaticCallableOwner::install(2);
echo StaticCallableOwner::invoke($first), "|";
unset($first);
$second = StaticCallableOwner::$shared;
StaticCallableOwner::borrow($second);
echo StaticCallableOwner::invoke($second), "|";
unset($second);
"#,
        "1|drop1|2|drop2|",
    );
}

/// A static callable property assigned from itself keeps a balanced retain/release: the acquired
/// independent owner is published before the previous descriptor is retired, so repeated
/// self-assignment neither leaks nor frees the live descriptor.
#[test]
fn test_static_callable_property_self_assignment_stays_balanced() {
    assert_static_callable_output(
        r#"<?php
class StaticSelfAssign {
    public static $shared;
    public static function install(int $number): void {
        self::$shared = static fn(): int => $number;
    }
    public static function copyOntoItself(): void {
        self::$shared = self::$shared;
    }
    public static function invoke(callable $callback): int { return $callback(); }
}
StaticSelfAssign::install(5);
StaticSelfAssign::copyOntoItself();
echo StaticSelfAssign::invoke(StaticSelfAssign::$shared), "|";
StaticSelfAssign::copyOntoItself();
echo StaticSelfAssign::invoke(StaticSelfAssign::$shared), "|";
"#,
        "5|5|",
    );
}

/// Overwriting a static callable property whose released descriptor runs a captured-object
/// destructor that reentrantly stores a new descriptor must keep the reentrant store: the
/// replacement is published before the old descriptor is retired, and the reentrant value is not
/// overwritten once the release helper returns.
#[test]
fn test_static_callable_property_reentrant_destructor_store_survives() {
    assert_static_callable_output(
        r#"<?php
class ReentrantDropper {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() {
        ReentrantStaticStore::$shared = static fn(): int => 99;
    }
}
class ReentrantStaticStore {
    public static $shared;
    public static function installTracked(int $number): void {
        $tracker = new ReentrantDropper($number);
        self::$shared = static function () use ($tracker): int { return $tracker->id; };
    }
    public static function installPlain(int $number): void {
        self::$shared = static fn(): int => $number;
    }
    public static function invoke(callable $callback): int { return $callback(); }
}
ReentrantStaticStore::installTracked(1);
ReentrantStaticStore::installPlain(2);
echo ReentrantStaticStore::invoke(ReentrantStaticStore::$shared), "|";
"#,
        "99|",
    );
}
