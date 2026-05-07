use crate::support::*;

#[test]
fn test_fiber_construction_does_not_crash() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_fiber_state_predicates_initial() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
if ($f->isStarted()) { echo "S"; } else { echo "s"; }
if ($f->isRunning()) { echo "R"; } else { echo "r"; }
if ($f->isSuspended()) { echo "P"; } else { echo "p"; }
if ($f->isTerminated()) { echo "T"; } else { echo "t"; }
"#,
    );
    assert_eq!(out, "srpt");
}

#[test]
fn test_fiber_get_current_returns_null_outside_fiber() {
    // From outside any fiber, getCurrent returns the boxed null Mixed cell.
    // is_null() narrows back to a clean boolean we can assert on.
    let out = compile_and_run(
        r#"<?php
echo is_null(Fiber::getCurrent()) ? "null" : "not-null";
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_fiber_get_current_inside_is_boxed_fiber_object() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    $cur = Fiber::getCurrent();
    echo ($cur instanceof Fiber) ? "fiber" : "not-fiber";
    echo "/";
    echo $cur->isRunning() ? "running" : "not-running";
});
$f->start();
"#,
    );
    assert_eq!(out, "fiber/running");
}

#[test]
fn test_fiber_runs_to_completion() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void { echo "inside"; });
$f->start();
echo "|after";
if ($f->isTerminated()) { echo "|term"; }
"#,
    );
    assert_eq!(out, "inside|after|term");
}

#[test]
fn test_fiber_suspend_returns_value_to_caller() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend(42);
});
$r = $f->start();
echo $r;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_fiber_suspend_without_value_yields_null() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend();
});
$r = $f->start();
echo is_null($r) ? "null" : "not-null";
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_fiber_resume_delivers_value_to_suspend() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    $v = Fiber::suspend(0);
    echo "got=" . $v;
});
$f->start();
$f->resume(99);
"#,
    );
    assert_eq!(out, "got=99");
}

#[test]
fn test_fiber_full_suspend_resume_cycle() {
    // Mixed-tagged values flow through `transfer_value`. We echo each
    // received Mixed payload directly without arithmetic so the test does
    // not depend on Mixed-cell arithmetic auto-unboxing.
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    $a = Fiber::suspend("yield-1");
    echo "[got " . $a . "]";
    $b = Fiber::suspend("yield-2");
    echo "[got " . $b . "]";
    Fiber::suspend("yield-3");
});
echo $f->start();
echo "|";
echo $f->resume("resume-A");
echo "|";
echo $f->resume("resume-B");
"#,
    );
    assert_eq!(out, "yield-1|[got resume-A]yield-2|[got resume-B]yield-3");
}

#[test]
fn test_fiber_terminal_return_available_only_from_get_return() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): mixed {
    return "ret";
});
$v = $f->start();
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "null/ret");
}

#[test]
fn test_fiber_resume_returns_null_when_fiber_terminates() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): mixed {
    Fiber::suspend("yield");
    return "ret";
});
echo $f->start();
echo "/";
$v = $f->resume("go");
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "yield/null/ret");
}

#[test]
fn test_fiber_int_return_is_boxed_for_get_return() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): int {
    return 42;
});
$v = $f->start();
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "null/42");
}

#[test]
fn test_fiber_stack_is_released_when_object_is_freed() {
    // Each fiber owns a 256 KB stack from the heap (default 8 MB). Reassigning
    // $f over 50 iterations drops the previous Fiber's refcount to 0; the
    // object_free_deep hook must release its stack or we run out of heap
    // before iteration 32 and abort with "Fatal error: heap memory exhausted".
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 50; $i++) {
    $f = new Fiber(function(): void {});
    $f->start();
}
echo "iters=" . $i;
"#,
    );
    assert_eq!(out, "iters=50");
}

#[test]
fn test_fiber_error_on_suspend_outside_fiber() {
    let out = compile_and_run(
        r#"<?php
try { Fiber::suspend(0); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot suspend outside of a fiber");
}

#[test]
fn test_fiber_error_on_start_twice() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
$f->start();
try { $f->start(); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot start a fiber that has already been started");
}

#[test]
fn test_fiber_error_on_resume_terminated() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
$f->start();
try { $f->resume(0); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot resume a fiber that is not suspended");
}

#[test]
fn test_fiber_error_on_get_return_before_terminated() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void { Fiber::suspend(0); });
$f->start();
try { $f->getReturn(); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot get fiber return value: The fiber has not returned");
}

#[test]
fn test_fiber_error_on_throw_not_suspended() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
try { $f->throw(new FiberError("x")); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot resume a fiber that is not suspended");
}

#[test]
fn test_fiber_uncaught_exception_escapes_to_caller() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    throw new Exception("from fiber");
});
try { $f->start(); echo "no-throw"; }
catch (Exception $e) { echo "caught:" . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "caught:from fiber");
}

#[test]
fn test_fiber_throw_escapes_when_fiber_does_not_catch() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend(0);
});
$f->start();
try { $f->throw(new Exception("via throw")); echo "no-throw"; }
catch (Exception $e) { echo "caught:" . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "caught:via throw");
}

#[test]
fn test_fiber_internal_catch_does_not_escape() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    try { throw new Exception("internal"); }
    catch (Exception $e) { echo "fiber-caught;"; }
});
$f->start();
echo "after-start";
"#,
    );
    assert_eq!(out, "fiber-caught;after-start");
}

#[test]
fn test_fiber_start_passes_arguments_to_closure() {
    // start(...$args) hands up to four Mixed payloads to the closure. The
    // Fiber entry wrapper adapts those cells to the closure's declared ABI.
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(mixed $a, mixed $b): void {
    echo $a . "/" . $b;
});
$f->start("hello", "world");
"#,
    );
    assert_eq!(out, "hello/world");
}

#[test]
fn test_fiber_start_untyped_argument_receives_value() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function($x): void {
    echo $x;
});
$f->start(42);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_fiber_from_closure_variable_uses_entry_wrapper() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x): int {
    echo $x;
    return $x + 1;
};
$f = new Fiber($fn);
$v = $f->start(41);
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "41/null/42");
}

#[test]
fn test_fiber_start_typed_int_argument_receives_value() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(int $x): void {
    echo $x + 1;
});
$f->start(41);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_fiber_start_typed_float_argument_receives_value() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(float $x): void {
    echo $x + 0.5;
});
$f->start(1.5);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_fiber_start_typed_string_arguments_use_stack_overflow() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(string $a, string $b, string $c, string $d, string $e): void {
    echo $a . $b . $c . $d . $e;
});
$f->start("A", "B", "C", "D", "E");
"#,
    );
    assert_eq!(out, "ABCDE");
}

#[test]
fn test_fiber_start_typed_argument_with_string_capture() {
    let out = compile_and_run(
        r#"<?php
$suffix = "!";
$f = new Fiber(function(int $x) use ($suffix): void {
    echo ($x + 1) . $suffix;
});
$f->start(41);
"#,
    );
    assert_eq!(out, "42!");
}

#[test]
fn test_fiber_first_class_function_callable_uses_entry_wrapper() {
    let out = compile_and_run(
        r#"<?php
function fiber_job(int $x): int {
    echo $x;
    return $x + 1;
}
$f = new Fiber(fiber_job(...));
$v = $f->start(41);
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "41/null/42");
}

#[test]
fn test_fiber_start_seven_args() {
    // 7 = the maximum, equal to the AArch64 integer arg-reg count minus $this.
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(mixed $a, mixed $b, mixed $c, mixed $d, mixed $e, mixed $g, mixed $h): void {
    echo $a . $b . $c . $d . $e . $g . $h;
});
$f->start("1", "2", "3", "4", "5", "6", "7");
"#,
    );
    assert_eq!(out, "1234567");
}

#[test]
fn test_fiber_start_zero_one_four_args() {
    let out = compile_and_run(
        r#"<?php
$f0 = new Fiber(function(): void { echo "[0]"; });
$f0->start();
$f1 = new Fiber(function(mixed $a): void { echo "[1:" . $a . "]"; });
$f1->start("one");
$f4 = new Fiber(function(mixed $a, mixed $b, mixed $c, mixed $d): void {
    echo "[4:" . $a . "," . $b . "," . $c . "," . $d . "]";
});
$f4->start("A", "B", "C", "D");
"#,
    );
    assert_eq!(out, "[0][1:one][4:A,B,C,D]");
}

#[test]
fn test_fiber_stack_overflow_faults_via_guard_page() {
    // The fiber stack now has a 16 KB PROT_NONE guard page at its bottom. A
    // runaway recursion inside the fiber must trip that page and terminate
    // with SIGSEGV/SIGBUS instead of silently corrupting the heap.
    let out = compile_and_run_capture(
        r#"<?php
function recurse(int $n): int {
    $buf = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    if ($n > 1000000) return $n;
    return recurse($n + 1);
}
$f = new Fiber(function(): void {
    recurse(0);
});
$f->start();
echo "should-not-reach";
"#,
    );
    assert!(!out.success, "expected the stack overflow to abort the program");
    assert!(
        !out.stdout.contains("should-not-reach"),
        "control should not return after the guard fault, stdout was: {}",
        out.stdout
    );
}

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

#[test]
fn test_fiber_php_constructs_inside_body() {
    // try/finally, foreach, match, and `new` all work inside a fiber's
    // closure body, including across a suspend boundary in the try block.
    let out = compile_and_run(
        r#"<?php
class Item { public string $name = ""; }
$f = new Fiber(function(): void {
    try {
        echo "T;";
        Fiber::suspend(0);
        echo "A;";
    } finally {
        echo "F;";
    }
    $items = [10, 20, 30];
    foreach ($items as $v) { echo "v" . $v . ";"; }
    $x = 2;
    $r = match ($x) { 1 => "one", 2 => "two", default => "other" };
    echo "m" . $r . ";";
    $i = new Item();
    $i->name = "widget";
    echo "o" . $i->name;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "T;A;F;v10;v20;v30;mtwo;owidget");
}

#[test]
fn test_fiber_canonical_php_doc_example() {
    // The canonical example from the PHP documentation, adapted for elephc's
    // mixed-payload-only suspend/resume surface (strings round-trip cleanly).
    let out = compile_and_run(
        r#"<?php
$fiber = new Fiber(function(): void {
    $value = Fiber::suspend("fiber");
    echo "Value used to resume fiber: " . $value;
});
$value = $fiber->start();
echo "Value from fiber suspending: " . $value . "|";
$fiber->resume("test");
"#,
    );
    assert_eq!(out, "Value from fiber suspending: fiber|Value used to resume fiber: test");
}

#[test]
fn test_fiber_closure_capture_string_survives_suspend() {
    let out = compile_and_run(
        r#"<?php
$ctx = "stable";
$f = new Fiber(function() use ($ctx): void {
    Fiber::suspend(0);
    echo "after=" . $ctx;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "after=stable");
}

#[test]
fn test_fiber_closure_capture_survives_suspend_resume() {
    let out = compile_and_run(
        r#"<?php
$base = 100;
$f = new Fiber(function() use ($base): void {
    Fiber::suspend(0);
    echo "after-resume base=" . $base;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "after-resume base=100");
}

#[test]
fn test_fiber_string_payload_round_trip() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend("hello");
});
echo $f->start();
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_fiber_state_transitions() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void { Fiber::suspend(0); });
echo $f->isStarted() ? "S" : "s";
$f->start();
echo $f->isStarted() ? "S" : "s";
echo $f->isSuspended() ? "P" : "p";
echo $f->isTerminated() ? "T" : "t";
$f->resume(0);
echo $f->isTerminated() ? "T" : "t";
"#,
    );
    assert_eq!(out, "sSPtT");
}

#[test]
fn test_fiber_error_subclasses_exception() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new FiberError("nope");
} catch (Exception $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

#[test]
fn test_fiber_error_caught_by_specific_type() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new FiberError("x");
} catch (FiberError $e) {
    echo "fiber-err";
} catch (Exception $e) {
    echo "exc";
}
"#,
    );
    assert_eq!(out, "fiber-err");
}

#[test]
fn test_fiber_throw_caught_by_internal_try_catch() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    echo "1";
    try {
        Fiber::suspend(0);
        echo "X-not-reached";
    } catch (FiberError $e) {
        echo "2";
    }
    echo "3";
});
echo "A";
$f->start();
echo "B";
$f->throw(new FiberError("delivered"));
echo "C";
"#,
    );
    assert_eq!(out, "A1B23C");
}
