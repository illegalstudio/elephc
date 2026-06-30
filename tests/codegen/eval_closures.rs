//! Purpose:
//! End-to-end regressions for closure literals executed inside runtime eval.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_closure` through Rust's test harness.
//!
//! Key details:
//! - Fixtures compile PHP to native code, enter the eval bridge, and execute
//!   closure callable paths through elephc-magician.

use crate::support::{compile_and_run, compile_and_run_capture};

/// Verifies eval closure literals dispatch through direct calls and call_user_func_array.
#[test]
fn test_eval_closure_literal_dispatches_direct_and_call_user_func_array() {
    let out = compile_and_run(
        r#"<?php
eval('$fn = function($left, $right = 2) { return $left + $right; };
echo $fn(3); echo ":";
echo call_user_func_array($fn, ["right" => 6, "left" => 5]);');
"#,
    );

    assert_eq!(out, "5:11");
}

/// Verifies eval closure literals are exposed as PHP `Closure` objects.
#[test]
fn test_eval_closure_literal_is_php_closure_object() {
    let out = compile_and_run(
        r#"<?php
eval('$fn = function() { return "ok"; };
echo is_object($fn) ? "O" : "o"; echo ":";
echo get_class($fn); echo ":";
echo $fn instanceof Closure ? "I" : "i"; echo ":";
echo class_exists("Closure") ? "K" : "k"; echo ":";
echo is_callable($fn) ? "C" : "c"; echo ":";
echo call_user_func($fn);');
"#,
    );

    assert_eq!(out, "O:Closure:I:K:C:ok");
}

/// Verifies eval closure by-value captures snapshot the defining value for each call.
#[test]
fn test_eval_closure_by_value_capture_uses_snapshot() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 1;
$fn = function($add) use ($x) { $x += $add; return $x; };
$x = 9;
echo $fn(1); echo ":";
echo $fn(2); echo ":";
echo $x;');
"#,
    );

    assert_eq!(out, "2:3:9");
}

/// Verifies eval closure by-reference captures write back to the defining scope.
#[test]
fn test_eval_closure_by_ref_capture_writes_back() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 1;
$fn = function() use (&$x) { $x += 4; };
$fn();
echo $x;');
"#,
    );

    assert_eq!(out, "5");
}

/// Verifies eval closure literals are visible through ReflectionFunction metadata and invocation.
#[test]
fn test_eval_closure_reflection_function_metadata_and_invoke() {
    let out = compile_and_run(
        r#"<?php
eval('$seed = 4;
$fn = function($delta = 1) use ($seed) { return $seed + $delta; };
$ref = new ReflectionFunction($fn);
$staticFn = static function() {};
$staticRef = new ReflectionFunction($staticFn);
echo $ref->isClosure() ? "C" : "c"; echo ":";
echo $ref->isAnonymous() ? "A" : "a"; echo ":";
echo $ref->isStatic() ? "S" : "s"; echo ":";
echo $staticRef->isClosure() ? "C" : "c"; echo ":";
echo $staticRef->isStatic() ? "S" : "s"; echo ":";
$vars = $ref->getClosureUsedVariables();
echo count($vars); echo ":";
echo $vars["seed"]; echo ":";
echo $ref->invoke(3); echo ":";
echo $ref->invokeArgs(["delta" => 5]);');
"#,
    );

    assert_eq!(out, "C:A:s:C:S:1:4:7:9");
}

/// Verifies eval `Closure::call()` binds `$this` and preserves by-ref argument writeback.
#[test]
fn test_eval_closure_call_binds_this_and_writes_back_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureCallBox {
    public int $base = 10;
}
$box = new EvalClosureCallBox();
$fn = function(int &$value, int $delta): int {
    $value = $value + $this->base + $delta;
    return $value;
};
$seed = "2";
echo $fn->call($box, $seed, 3);
echo ":";
echo gettype($seed);
echo ":";
echo $seed;');
"#,
    );

    assert_eq!(out, "15:integer:15");
}

/// Verifies eval `Closure::bind()` and `bindTo()` persist `$this` across later calls.
#[test]
fn test_eval_closure_bind_persists_this_and_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureBindBox {
    public int $base = 10;
}
$box = new EvalClosureBindBox();
$fn = function(int &$value, int $delta): int {
    $value = $value + $this->base + $delta;
    return $value;
};

$bound = $fn->bindTo($box);
$seed = "2";
echo is_object($bound) ? "O:" : "o:";
echo $bound($seed, 3) . ":" . gettype($seed) . ":" . $seed . "|";

$other = "4";
echo call_user_func_array($bound, [&$other, 5]) . ":" . gettype($other) . ":" . $other . "|";

$staticBound = Closure::bind($fn, $box);
$third = "6";
echo $staticBound($third, 7) . ":" . gettype($third) . ":" . $third . "|";

$static = static function() { return "bad"; };
echo is_null($static->bindTo($box)) ? "N" : "n";');
"#,
    );

    assert_eq!(out, "O:15:integer:15|19:integer:19|23:integer:23|N");
}

/// Verifies eval `Closure::bind()` and `bindTo()` honor explicit private-access scope.
#[test]
fn test_eval_closure_bind_honors_explicit_scope_and_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureScopeBase {
    private int $secret = 40;
}

class EvalClosureScopeChild extends EvalClosureScopeBase {}

class EvalClosureScopeFactory {
    public function make() {
        return function(int &$value, int $delta): string {
            $value += $this->secret + $delta;
            return $value . ":" . get_called_class();
        };
    }
}

$fn = (new EvalClosureScopeFactory())->make();
$child = new EvalClosureScopeChild();

$bound = $fn->bindTo($child, "EvalClosureScopeBase");
$first = "1";
echo $bound($first, 1) . ":" . gettype($first) . ":" . $first . "|";

$staticBound = Closure::bind($fn, $child, "EvalClosureScopeBase");
$second = "2";
echo call_user_func_array($staticBound, [&$second, 2]) . ":" . gettype($second) . ":" . $second;');
"#,
    );

    assert_eq!(
        out,
        "42:EvalClosureScopeChild:integer:42|44:EvalClosureScopeChild:integer:44"
    );
}

/// Verifies eval Closure binding to `null` preserves explicit class scope and by-ref args.
#[test]
fn test_eval_closure_bind_null_receiver_preserves_explicit_scope_and_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureNullScopeBox {
    private static int $secret = 40;
}

$fn = function(int &$value, int $delta): int {
    $value += self::$secret + $delta;
    return $value;
};

$bound = Closure::bind($fn, null, "EvalClosureNullScopeBox");
$first = "1";
echo $bound($first, 2) . ":" . gettype($first) . ":" . $first . "|";

$boundTo = $fn->bindTo(null, "EvalClosureNullScopeBox");
$second = "3";
echo call_user_func_array($boundTo, [&$second, 4]) . ":" . gettype($second) . ":" . $second;');
"#,
    );

    assert_eq!(out, "43:integer:43|47:integer:47");
}

/// Verifies eval Closure `__invoke` works as an array callable and preserves by-ref args.
#[test]
fn test_eval_closure_invoke_array_callable_preserves_by_ref_args() {
    let out = compile_and_run_capture(
        r#"<?php
eval('$fn = function(int &$value, int $delta): int {
    $value += $delta;
    return $value;
};
$callback = [$fn, "__invoke"];
echo is_callable($callback) ? "C:" : "c:";

$first = "2";
echo $callback($first, 3) . ":" . gettype($first) . ":" . $first . "|";

$second = "4";
echo call_user_func_array($callback, [&$second, 5]) . ":" . gettype($second) . ":" . $second . "|";

$fromCallable = Closure::fromCallable($callback);
$third = "6";
echo $fromCallable($third, 7) . ":" . gettype($third) . ":" . $third . "|";

$fourth = "8";
echo $fn->__invoke($fourth, 9) . ":" . gettype($fourth) . ":" . $fourth;');
"#,
    );

    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "C:5:integer:5|9:integer:9|13:integer:13|17:integer:17"
    );
}
