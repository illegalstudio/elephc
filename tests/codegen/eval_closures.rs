//! Purpose:
//! End-to-end regressions for closure literals executed inside runtime eval.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_closure` through Rust's test harness.
//!
//! Key details:
//! - Fixtures compile PHP to native code, enter the eval bridge, and execute
//!   closure callable paths through elephc-magician.

use crate::support::compile_and_run;

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
