//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers captures, including fiber closure capture integer, fiber closure capture two ints, and fiber closure capture three ints.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_fiber_closure_capture_int() {
    let out = compile_and_run(
        r#"<?php
$ctx = 42;
$f = new Fiber(function() use ($ctx): void { echo "got=" . $ctx; });
$f->start();
"#,
    );
    assert_eq!(out, "got=42");
}

#[test]
fn test_fiber_closure_capture_two_ints() {
    let out = compile_and_run(
        r#"<?php
$a = 10;
$b = 32;
$f = new Fiber(function() use ($a, $b): void { echo $a + $b; });
$f->start();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_fiber_closure_capture_three_ints() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$y = 2;
$z = 3;
$f = new Fiber(function() use ($x, $y, $z): void { echo $x + $y + $z; });
$f->start();
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_fiber_closure_capture_with_user_arg() {
    // The user arg must be typed `mixed` to round-trip cleanly through
    // start_args; the capture rides in start_args[1] and stays untouched
    // because user_arg_max is lowered to 1.
    let out = compile_and_run(
        r#"<?php
$mul = 3;
$f = new Fiber(function(mixed $x) use ($mul): void { echo "x=" . $x . ",mul=" . $mul; });
$f->start(7);
"#,
    );
    assert_eq!(out, "x=7,mul=3");
}

#[test]
fn test_fiber_closure_capture_string() {
    let out = compile_and_run(
        r#"<?php
$s = "hello";
$f = new Fiber(function() use ($s): void { echo "got=" . $s; });
$f->start();
"#,
    );
    assert_eq!(out, "got=hello");
}

#[test]
fn test_fiber_closure_capture_two_strings() {
    let out = compile_and_run(
        r#"<?php
$a = "foo";
$b = "bar";
$f = new Fiber(function() use ($a, $b): void { echo $a . "/" . $b; });
$f->start();
"#,
    );
    assert_eq!(out, "foo/bar");
}

#[test]
fn test_fiber_closure_capture_int_then_string() {
    let out = compile_and_run(
        r#"<?php
$n = 42;
$s = "answer";
$f = new Fiber(function() use ($n, $s): void { echo $n . "=" . $s; });
$f->start();
"#,
    );
    assert_eq!(out, "42=answer");
}

#[test]
fn test_fiber_closure_capture_string_then_int() {
    let out = compile_and_run(
        r#"<?php
$s = "value";
$n = 7;
$f = new Fiber(function() use ($s, $n): void { echo $s . ":" . $n; });
$f->start();
"#,
    );
    assert_eq!(out, "value:7");
}

#[test]
fn test_fiber_closure_capture_int_survives_caller_reassignment() {
    // The capture is incref'd at construction so the original value stays
    // reachable through the Fiber even when the caller's variable is later
    // reassigned. (Plain ints are stored by value, so this also exercises the
    // fact that the captured int snapshot is not dragged along by the
    // reassignment.)
    let out = compile_and_run(
        r#"<?php
$n = 42;
$f = new Fiber(function() use ($n): void { echo "captured=" . $n; });
$n = 99;
$f->start();
"#,
    );
    assert_eq!(out, "captured=42");
}

#[test]
fn test_fiber_closure_capture_float() {
    let out = compile_and_run(
        r#"<?php
$pi = 3.14;
$f = new Fiber(function() use ($pi): void { echo "pi=" . $pi; });
$f->start();
"#,
    );
    assert_eq!(out, "pi=3.14");
}

#[test]
fn test_fiber_closure_capture_float_and_int() {
    let out = compile_and_run(
        r#"<?php
$rate = 0.5;
$count = 4;
$f = new Fiber(function() use ($rate, $count): void {
    echo "count=" . $count . " rate=" . $rate . " product=" . ($count * $rate);
});
$f->start();
"#,
    );
    assert_eq!(out, "count=4 rate=0.5 product=2");
}

#[test]
fn test_fiber_closure_capture_float_and_string() {
    let out = compile_and_run(
        r#"<?php
$factor = 2.5;
$tag = "result";
$f = new Fiber(function() use ($factor, $tag): void { echo $tag . "=" . $factor; });
$f->start();
"#,
    );
    assert_eq!(out, "result=2.5");
}

#[test]
fn test_fiber_closure_capture_two_floats() {
    let out = compile_and_run(
        r#"<?php
$a = 1.5;
$b = 2.5;
$f = new Fiber(function() use ($a, $b): void { echo "sum=" . ($a + $b); });
$f->start();
"#,
    );
    assert_eq!(out, "sum=4");
}

#[test]
fn test_fiber_closure_capture_object() {
    let out = compile_and_run(
        r#"<?php
class Counter { public int $value = 0; }
$c = new Counter();
$c->value = 100;
$f = new Fiber(function() use ($c): void { echo "v=" . $c->value; });
$f->start();
"#,
    );
    assert_eq!(out, "v=100");
}

#[test]
fn test_fiber_closure_capture_object_mutation_visible_to_caller() {
    // Objects are reference types in PHP — when the fiber mutates a captured
    // object, the change is visible to the original caller because both share
    // the same heap object. The capture refcount keeps the object alive but
    // does not isolate it.
    let out = compile_and_run(
        r#"<?php
class Counter { public int $value = 0; }
$d = new Counter();
$d->value = 5;
$f = new Fiber(function() use ($d): void { $d->value = $d->value + 100; });
$f->start();
echo "after=" . $d->value;
"#,
    );
    assert_eq!(out, "after=105");
}

#[test]
fn test_fiber_closure_capture_array() {
    let out = compile_and_run(
        r#"<?php
$arr = [10, 20, 30];
$f = new Fiber(function() use ($arr): void { echo $arr[0] . "/" . $arr[2]; });
$f->start();
"#,
    );
    assert_eq!(out, "10/30");
}

#[test]
fn test_fiber_closure_capture_array_survives_caller_reassignment() {
    // The captured array stays alive across reassignment of the caller's $arr
    // because emit_fiber_capture_preload incref's the heap pointer at capture
    // time. Without the incref, reassigning $arr would drop the original
    // array's refcount to zero and free it before the fiber consumed it.
    let out = compile_and_run(
        r#"<?php
$arr = [10, 20, 30];
$f = new Fiber(function() use ($arr): void {
    echo $arr[0] . "/" . $arr[1] . "/" . $arr[2];
});
$arr = [99];
$f->start();
"#,
    );
    assert_eq!(out, "10/20/30");
}

#[test]
fn test_fiber_multiple_fibers_share_captured_object() {
    // Two fibers each capture the same Counter object. Mutations from one
    // fiber are visible to the other and to main, because the capture stores
    // the shared heap pointer (with refcount bumped twice — once per fiber).
    let out = compile_and_run(
        r#"<?php
class Counter { public int $value = 0; }
$shared = new Counter();
$shared->value = 0;
$f1 = new Fiber(function() use ($shared): void {
    Fiber::suspend(0);
    $shared->value = $shared->value + 1;
});
$f2 = new Fiber(function() use ($shared): void {
    Fiber::suspend(0);
    $shared->value = $shared->value + 10;
});
$f1->start();
$f2->start();
$f1->resume(0);
$f2->resume(0);
echo $shared->value;
"#,
    );
    assert_eq!(out, "11");
}

#[test]
fn test_fiber_nested_with_outer_capture_passed_to_inner() {
    // Inside the outer fiber's body, $shared is a local visible from a
    // capture; passing it as a capture to the inner fiber re-triggers the
    // capture preload path and the inner closure sees the same value.
    let out = compile_and_run(
        r#"<?php
$shared = 100;
$outer = new Fiber(function() use ($shared): void {
    $inner = new Fiber(function() use ($shared): void { echo "inner=" . $shared; });
    $inner->start();
});
$outer->start();
"#,
    );
    assert_eq!(out, "inner=100");
}

