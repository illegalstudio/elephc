//! Purpose:
//! Regression tests for untyped parameters that receive a closure at one call site and a
//! different kind of value at another.
//!
//! Called from:
//! - The codegen integration harness through the callables module.
//!
//! Key details:
//! - Call-site specialization used to skip closure arguments when widening an untyped
//!   parameter, so a parameter first called with an object or a string callable kept that
//!   type and the body read the closure descriptor as an object or a function name.
//! - Expected output is verbatim `php` output; each fixture also runs under `--heap-debug`.

use crate::support::*;

/// A function parameter first called with an object, then a closure, keeps both apart.
#[test]
fn test_untyped_function_param_keeps_a_closure_after_an_object() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ClosureParamFoo {}
function closure_param_show($v) { echo is_object($v) ? get_class($v) : gettype($v), "\n"; }
closure_param_show(new ClosureParamFoo());
closure_param_show(function () {});
closure_param_show(strlen(...));
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ClosureParamFoo\nClosure\nClosure\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A string callable followed by a closure still calls each one.
#[test]
fn test_untyped_function_param_calls_a_closure_after_a_string_callable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function closure_param_apply($f, $x) { return $f($x); }
echo closure_param_apply('abs', -3), "\n";
echo closure_param_apply(fn($x) => $x * 2, 4), "\n";
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3\n8\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Instance and static method parameters widen the same way as function parameters.
#[test]
fn test_untyped_method_params_keep_a_closure_after_an_object() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ClosureParamTaker {
    public function take($v) { echo $v instanceof Closure ? "closure" : get_class($v), "\n"; }
    public static function stake($v) { echo $v instanceof Closure ? "closure" : get_class($v), "\n"; }
}
$taker = new ClosureParamTaker();
$taker->take(new ClosureParamTaker());
$taker->take(fn() => 1);
ClosureParamTaker::stake(new ClosureParamTaker());
ClosureParamTaker::stake(fn() => 1);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ClosureParamTaker\nclosure\nClosureParamTaker\nclosure\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
