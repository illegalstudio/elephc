//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers arguments, including fiber start passes arguments to closure, fiber start untyped argument receives value, and fiber from closure variable uses entry wrapper.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that `$f->start($a, $b)` forwards two `mixed` arguments to the fiber
/// closure's parameters and formats them correctly.
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

/// Verifies that an untyped (no type declaration) fiber closure parameter receives
/// the value passed to `$f->start()` and `echo` outputs it correctly.
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

/// Verifies that a Fiber constructed from a closure variable (not an inline
/// closure literal) works correctly: arguments are received, `start()` returns
/// null (first-class callables don't capture return values that way), and
/// `getReturn()` returns the fiber's final return value.
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

/// Verifies that an `int`-typed fiber closure parameter receives a value via
/// `start()` and the fiber body can perform arithmetic on it.
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

/// Verifies that a `float`-typed fiber closure parameter receives a value via
/// `start()` and the fiber body correctly performs floating-point arithmetic.
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

/// Verifies that five `string`-typed parameters are all received correctly when
/// passed via `start()` and can be concatenated in the fiber body.
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

/// Verifies that an `int`-typed fiber closure parameter combined with a `use`
/// captured variable works correctly: the parameter is received from `start()`
/// and the captured variable is accessible in the fiber body.
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

/// Verifies that a Fiber constructed from a first-class callable (`fiber_job(...)`)
/// works correctly: arguments are received, `start()` returns null, and
/// `getReturn()` returns the fiber's final return value.
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

/// Verifies that a runtime string user-function callback is materialized as a Fiber descriptor.
#[test]
fn test_fiber_string_user_function_callable_uses_descriptor_invoker() {
    let out = compile_and_run(
        r#"<?php
function fiber_string_job(int $value): int {
    echo $value;
    return $value + 1;
}

$callback = "fiber_string_job";
$f = new Fiber($callback);
$v = $f->start(41);
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "41/null/42");
}

/// Verifies that a builtin string callback can run as a Fiber through descriptor metadata.
#[test]
fn test_fiber_string_builtin_callable_uses_descriptor_invoker() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber("STRLEN");
$v = $f->start("hello");
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "null/5");
}

/// Verifies that a declared extern string callback can run as a Fiber descriptor.
#[test]
fn test_fiber_string_extern_callable_uses_descriptor_invoker() {
    let out = compile_and_run(
        r#"<?php
extern function atoi(string $value): int;

$callback = "ATOI";
$f = new Fiber($callback);
$f->start("42");
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a static-method callable-array literal is materialized for Fiber invocation.
#[test]
fn test_fiber_static_callable_array_literal_uses_descriptor_invoker() {
    let out = compile_and_run(
        r#"<?php
class FiberStaticJob {
    public static function run(string $value): string {
        echo "static:" . $value;
        return "done";
    }
}

$f = new Fiber([FiberStaticJob::class, "run"]);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "static:go/null/done");
}

/// Verifies that an instance-method callable-array literal stores its receiver in the descriptor.
#[test]
fn test_fiber_instance_callable_array_literal_uses_descriptor_receiver() {
    let out = compile_and_run(
        r#"<?php
class FiberArrayJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$job = new FiberArrayJob("array:");
$f = new Fiber([$job, "run"]);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "array:go/null/array:done");
}

/// Verifies an inline object receiver in a Fiber callable-array literal is captured once.
#[test]
fn test_fiber_instance_callable_array_literal_accepts_inline_receiver() {
    let out = compile_and_run(
        r#"<?php
class FiberInlineArrayJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$f = new Fiber([new FiberInlineArrayJob("inline:"), "run"]);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "inline:go/null/inline:done");
}

/// Verifies that a stored instance-method callable array captures the receiver from slot zero.
#[test]
fn test_fiber_stored_instance_callable_array_uses_stored_receiver() {
    let out = compile_and_run(
        r#"<?php
class FiberStoredArrayJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$first = new FiberStoredArrayJob("first:");
$cb = [$first, "run"];
$fiber = new Fiber($cb);
$cb = [new FiberStoredArrayJob("second:"), "run"];
$v = $fiber->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $fiber->getReturn();
"#,
    );
    assert_eq!(out, "first:go/null/first:done");
}

/// Verifies a runtime-selected instance callable-array variable is converted to a Fiber descriptor.
#[test]
fn test_fiber_runtime_selected_instance_callable_array_variable() {
    let out = compile_and_run(
        r#"<?php
class FiberRuntimeArrayJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$first = new FiberRuntimeArrayJob("runtime:");
$second = new FiberRuntimeArrayJob("bad:");
$method = "run";
$callback = [$first, $method];
$fiber = new Fiber($callback);
$callback = [$second, $method];
$v = $fiber->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $fiber->getReturn();
"#,
    );
    assert_eq!(out, "runtime:go/null/runtime:done");
}

/// Verifies a runtime-selected callable-array literal can provide a Fiber instance receiver.
#[test]
fn test_fiber_runtime_selected_instance_callable_array_literal() {
    let out = compile_and_run(
        r#"<?php
class FiberRuntimeLiteralJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$method = "run";
$f = new Fiber([new FiberRuntimeLiteralJob("literal:"), $method]);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "literal:go/null/literal:done");
}

/// Verifies a runtime-selected static callable-array literal can start a Fiber.
#[test]
fn test_fiber_runtime_selected_static_callable_array_literal() {
    let out = compile_and_run(
        r#"<?php
class FiberRuntimeStaticJob {
    public static function run(string $value): string {
        echo "static:" . $value;
        return "static:done";
    }
}

$class = FiberRuntimeStaticJob::class;
$method = "run";
$f = new Fiber([$class, $method]);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "static:go/null/static:done");
}

/// Verifies that an invokable object variable is converted into a Fiber descriptor.
#[test]
fn test_fiber_invokable_object_callable_uses_descriptor_receiver() {
    let out = compile_and_run(
        r#"<?php
class FiberInvokerJob {
    public function __construct(private string $prefix) {}

    public function __invoke(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$job = new FiberInvokerJob("invoke:");
$f = new Fiber($job);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "invoke:go/null/invoke:done");
}

/// Verifies an inline invokable object is captured as a Fiber descriptor receiver.
#[test]
fn test_fiber_invokable_object_literal_uses_descriptor_receiver() {
    let out = compile_and_run(
        r#"<?php
class FiberInlineInvokerJob {
    public function __construct(private string $prefix) {}

    public function __invoke(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}

$f = new Fiber(new FiberInlineInvokerJob("invoke-inline:"));
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "invoke-inline:go/null/invoke-inline:done");
}

/// Verifies that an inline variadic Fiber closure receives all start args in `...$args`.
#[test]
fn test_fiber_variadic_inline_closure_receives_start_args() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(...$args): int {
    echo count($args) . ":" . $args[0] . "/" . $args[2];
    return count($args);
});
$v = $f->start("a", "b", "c");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "3:a/c/null/3");
}

/// Verifies that a variadic closure variable receives fixed and tail Fiber start args.
#[test]
fn test_fiber_variadic_closure_variable_builds_tail_array() {
    let out = compile_and_run(
        r#"<?php
$fn = function($head, ...$tail): int {
    echo $head . ":" . count($tail) . ":" . $tail[1];
    return count($tail);
};
$f = new Fiber($fn);
$v = $f->start("h", "a", "b");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "h:2:b/null/2");
}

/// Verifies that a variadic first-class callable receives Fiber start args through its wrapper.
#[test]
fn test_fiber_variadic_first_class_callable_builds_tail_array() {
    let out = compile_and_run(
        r#"<?php
function fiber_variadic_job(string $prefix, ...$items): int {
    echo $prefix . count($items);
    return count($items);
}

$f = new Fiber(fiber_variadic_job(...));
$v = $f->start("p", 4, 5);
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "p2/null/2");
}

/// Verifies an associative spread passed to `Fiber::start()` maps string keys
/// onto the fiber callback's fixed parameters, not synthetic start() slot names.
#[test]
fn test_fiber_start_assoc_spread_maps_named_callback_params() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(mixed $a, mixed $b, mixed $c): void {
    echo gettype($a) . ":" . $a . "\n";
    echo gettype($b) . ":" . $b . "\n";
    echo gettype($c) . ":" . $c . "\n";
});
$args = ["b" => "bee", "a" => 7, "c" => 9];
$f->start(...$args);
"#,
    );
    assert_eq!(out, "integer:7\nstring:bee\ninteger:9\n");
}

/// Verifies `Fiber::start(...$assoc)` preserves named callback mapping while
/// `resume()` delivers a scalar payload back to the suspended fiber.
#[test]
fn test_fiber_start_assoc_spread_then_resume_scalar_round_trip() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(mixed $a, mixed $b, mixed $c): void {
    echo $a . ":" . $b . ":" . $c . "\n";
    $x = Fiber::suspend("ready");
    echo gettype($x) . ":" . $x;
});
$args = ["c" => "see", "a" => "aye", "b" => "bee"];
echo $f->start(...$args) . "\n";
$f->resume("done");
"#,
    );
    assert_eq!(out, "aye:bee:see\nready\nstring:done");
}

/// Verifies `Fiber::start(...$assoc)` keeps array payloads boxed and owned
/// correctly across the fiber start/suspend boundary.
#[test]
fn test_fiber_start_assoc_spread_preserves_array_payload() {
    let out = compile_and_run(
        r#"<?php
function make(): Fiber {
    return new Fiber(function(mixed $a, mixed $b, mixed $c): void {
        echo gettype($a) . ":" . $a . "\n";
        echo gettype($b) . ":" . $b . "\n";
        $x = Fiber::suspend($c);
        echo gettype($x) . ":" . $x . "\n";
    });
}

$args = ["b" => "bee", "a" => 7, "c" => ["k" => 1]];
$f = make();
$ret = $f->start(...$args);
echo $ret["k"] . "\n";
$f->resume("done");
"#,
    );
    assert_eq!(out, "integer:7\nstring:bee\n1\nstring:done\n");
}

/// Verifies that seven `mixed` arguments can be passed through `start()` to the
/// fiber closure.  Seven is the maximum on AArch64 (integer arg-reg count
/// minus `$this`).
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

/// Verifies that `start()` correctly forwards zero, one, and four arguments to
/// three different fibers with matching parameter counts.
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

/// Verifies that a runaway recursion inside a fiber triggers the guard page at
/// the bottom of the fiber stack and aborts with SIGSEGV/SIGBUS rather than
/// silently corrupting the heap. The program must not reach "should-not-reach".
#[test]
fn test_fiber_stack_overflow_faults_via_guard_page() {
    // The fiber stack now has a 16 KB PROT_NONE guard page at its bottom. Keep
    // work per frame scalar-only and retain a post-call effect to prevent tail
    // recursion, so slow Wine hosts reach the guard page deterministically.
    let out = compile_and_run_capture(
        r#"<?php
function recurse(int $n): int {
    if ($n > 1000000) return $n;
    $result = recurse($n + 1);
    if ($result < 0) echo "unreachable";
    return $result;
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
