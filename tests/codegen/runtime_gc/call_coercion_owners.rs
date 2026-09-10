//! Purpose:
//! Verifies temporary argument boxes survive calls and retire before same-frame catches.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Concrete arrays require caller-owned boxes at declared PHP array boundaries.
//! - Multiple owners coexist with receiver, overflow and floating-point ABI arguments.

use crate::support::*;

/// Returned array and Mixed shadows survive caller-root retirement, including exceptional calls.
#[test]
fn test_core_user_call_shadow_arguments_retire_on_return_and_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class UserCallShadowOwner { public function __destruct() { echo "drop|"; } }
function forwardArrayShadow(array $items, bool $fail): array {
    if ($fail) { throw new RuntimeException("stop"); }
    return $items;
}
function forwardMixedShadow(mixed $items, bool $fail): mixed {
    if ($fail) { throw new RuntimeException("stop"); }
    return $items;
}
for ($i = 0; $i < 3; $i++) {
    $array = forwardArrayShadow([new UserCallShadowOwner()], false);
    echo count($array), ":";
    unset($array);
    try { forwardArrayShadow([new UserCallShadowOwner()], true); }
    catch (RuntimeException $error) { echo "caught|"; unset($error); }
    $mixed = forwardMixedShadow([new UserCallShadowOwner()], false);
    echo count($mixed), ":";
    unset($mixed);
    try { forwardMixedShadow([new UserCallShadowOwner()], true); }
    catch (RuntimeException $error) { echo "caught|"; unset($error); }
}
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}done", "1:drop|drop|caught|1:drop|drop|caught|".repeat(3)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Rooted array arguments do not renumber a later callable's passthrough ownership.
#[test]
fn test_core_user_call_partial_roots_preserve_later_callable_return() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class LaterArgumentOwner { public function __destruct() { echo "drop|"; } }
function keepLaterCallback(array $items, callable $callback): callable {
    echo count($items), ":";
    return $callback;
}
function makeLaterCallback(): callable {
    $owner = new LaterArgumentOwner();
    return function() use ($owner): void { echo "called|"; };
}
$callback = keepLaterCallback([str_repeat("x", 24)], makeLaterCallback());
$callback();
unset($callback);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:called|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Throwing direct, static and instance calls release implicit boxes without consuming input aliases.
#[test]
fn test_core_call_coercion_owners_unwind_with_overflow_and_float_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CoercionOwnerToken {
    public function __destruct() { echo "drop|"; }
}
function failCoercionCall(array $a, array $b, int $c, int $d, int $e, int $f,
                          int $g, int $h, int $i, int $j, float $fraction): void {
    echo count($a), ":", count($b), ":", $c + $d + $e + $f + $g + $h + $i + $j, ":", $fraction, ":";
    throw new RuntimeException("direct");
}
class CoercionOwnerReceiver {
    public static function failStatic(array $a, array $b, int $c, int $d, int $e, int $f,
                                      int $g, int $h, int $i, int $j, float $fraction): void {
        echo count($a), ":", count($b), ":", $c + $d + $e + $f + $g + $h + $i + $j, ":", $fraction, ":";
        throw new RuntimeException("static");
    }
    public function failMethod(array $a, array $b, int $c, int $d, int $e, int $f,
                               int $g, int $h, int $i, int $j, float $fraction): void {
        echo count($a), ":", count($b), ":", $c + $d + $e + $f + $g + $h + $i + $j, ":", $fraction, ":";
        throw new RuntimeException("method");
    }
}
function exerciseCoercionCall(int $kind): void {
    $a = ["token" => new CoercionOwnerToken()];
    $b = [10, 20];
    $alias = $a;
    $receiver = new CoercionOwnerReceiver();
    try {
        if ($kind === 0) { failCoercionCall($a, $b, 1, 2, 3, 4, 5, 6, 7, 8, 1.5); }
        elseif ($kind === 1) { CoercionOwnerReceiver::failStatic($a, $b, 1, 2, 3, 4, 5, 6, 7, 8, 1.5); }
        else { $receiver->failMethod($a, $b, 1, 2, 3, 4, 5, 6, 7, 8, 1.5); }
    } catch (RuntimeException $error) {
        echo $error->getMessage(), "|";
        unset($error);
    }
    unset($a, $b, $receiver);
    echo count($alias), ":";
    unset($alias);
}
exerciseCoercionCall(0);
exerciseCoercionCall(1);
exerciseCoercionCall(2);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:2:36:1.5:direct|1:drop|1:2:36:1.5:static|1:drop|1:2:36:1.5:method|1:drop|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Normal returns detach coercion records before a subsequent throw unwinds the same activation.
#[test]
fn test_core_call_coercion_owners_detach_after_normal_return() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function countCoercionInputs(array $left, array $right): int {
    return count($left) + count($right);
}
$left = [1, 2];
$right = ["key" => 3];
for ($i = 0; $i < 8; $i++) {
    try {
        echo countCoercionInputs($left, $right);
        throw new RuntimeException("after");
    } catch (RuntimeException $error) { unset($error); }
}
echo ":", $left[0], ":", $right["key"];
unset($left, $right);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "33333333:1:3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
