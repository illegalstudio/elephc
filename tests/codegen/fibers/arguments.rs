//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers arguments, including fiber start passes arguments to closure, fiber start untyped argument receives value, and fiber from closure variable uses entry wrapper.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

// Verifies that `$f->start($a, $b)` forwards two `mixed` arguments to the fiber
// closure's parameters and formats them correctly.
#[test]
fn test_fiber_start_passes_arguments_to_closure() {
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

// Verifies that an untyped (no type declaration) fiber closure parameter receives
// the value passed to `$f->start()` and `echo` outputs it correctly.
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

// Verifies that a Fiber constructed from a closure variable (not an inline
// closure literal) works correctly: arguments are received, `start()` returns
// null (first-class callables don't capture return values that way), and
// `getReturn()` returns the fiber's final return value.
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

// Verifies that an `int`-typed fiber closure parameter receives a value via
// `start()` and the fiber body can perform arithmetic on it.
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

// Verifies that a `float`-typed fiber closure parameter receives a value via
// `start()` and the fiber body correctly performs floating-point arithmetic.
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

// Verifies that five `string`-typed parameters are all received correctly when
// passed via `start()` and can be concatenated in the fiber body.
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

// Verifies that an `int`-typed fiber closure parameter combined with a `use`
// captured variable works correctly: the parameter is received from `start()`
// and the captured variable is accessible in the fiber body.
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

// Verifies that a Fiber constructed from a first-class callable (`fiber_job(...)`)
// works correctly: arguments are received, `start()` returns null, and
// `getReturn()` returns the fiber's final return value.
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

/// Verifies that a Fiber first-class method callable reloads its receiver from
/// the runtime callable descriptor instead of from Fiber start-argument slots.
#[test]
fn test_fiber_first_class_method_callable_uses_descriptor_receiver() {
    let out = compile_and_run(
        r#"<?php
class FiberJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$job = new FiberJob("fiber:");
$f = new Fiber($job->run(...));
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "fiber:go/null/fiber:done");
}

/// Verifies that a runtime-selected callable descriptor can start a Fiber through the uniform invoker.
#[test]
fn test_fiber_runtime_selected_method_callable_uses_descriptor_invoker() {
    let source = r#"<?php
class FiberBranchJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

function fiber_pick_right(): bool {
    return true;
}

$left = new FiberBranchJob("left:");
$right = new FiberBranchJob("right:");
$cb = fiber_pick_right() ? $right->run(...) : $left->run(...);
$f = new Fiber($cb);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "right:go/null/right:done");

    let dir = make_cli_test_dir("elephc_fiber_runtime_descriptor_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("fiber_descriptor_invoker") && user_asm.contains("callable_invoker"),
        "runtime-selected Fiber callbacks should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

// Verifies that seven `mixed` arguments can be passed through `start()` to the
// fiber closure.  Seven is the maximum on AArch64 (integer arg-reg count
// minus `$this`).
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

// Verifies that `start()` correctly forwards zero, one, and four arguments to
// three different fibers with matching parameter counts.
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

// Verifies that a runaway recursion inside a fiber triggers the guard page at
// the bottom of the fiber stack and aborts with SIGSEGV/SIGBUS rather than
// silently corrupting the heap. The program must not reach "should-not-reach".
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
