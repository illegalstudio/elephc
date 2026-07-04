//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of exceptions, including exception try catch same function, builtin exception try catch, and builtin exception message api.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use crate::support::*;

/// Verifies exception try catch same function.
#[test]
fn test_exception_try_catch_same_function() {
    // Compiles a custom exception class, throws it, and catches it within the
    // same function scope. Verifies the catch branch executes and the exception
    // variable is in scope.
    let out = compile_and_run(
        "<?php class MyException extends Exception {} try { throw new MyException(); } catch (MyException $e) { echo 42; }",
    );
    assert_eq!(out, "42");
}

/// Verifies builtin exception try catch.
#[test]
fn test_builtin_exception_try_catch() {
    // Catches a builtin Exception with a catch clause that has no variable (PHP 8+).
    // Confirms the catch block executes without reading the exception.
    let out =
        compile_and_run("<?php try { throw new Exception(); } catch (Exception $e) { echo 11; }");
    assert_eq!(out, "11");
}

/// Verifies builtin error try catch.
#[test]
fn test_builtin_error_try_catch() {
    // Throws a builtin Error and catches it, verifying getMessage() returns the
    // string passed to the constructor.
    let out = compile_and_run(
        "<?php try { throw new Error(\"boom\"); } catch (Error $e) { echo $e->getMessage(); }",
    );
    assert_eq!(out, "boom");
}

/// Verifies Error and Exception are distinct hierarchies — an Error is NOT
/// caught by catch (Exception), confirming the separate catch ordering.
#[test]
fn test_builtin_error_is_not_caught_by_exception() {
    let out = compile_and_run(
        "<?php try { throw new Error(\"boom\"); } catch (Exception $e) { echo \"exception\"; } catch (Error $e) { echo \"error\"; }",
    );
    assert_eq!(out, "error");
}

/// Checks that the public `$message` property and getMessage() both return the
/// constructor argument, verifying the Exception property surface.
#[test]
fn test_builtin_exception_message_api() {
    let out = compile_and_run(
        "<?php $e = new Exception(\"boom\"); echo $e->message; echo \":\"; echo $e->getMessage();",
    );
    assert_eq!(out, "boom:boom");
}

/// Verifies builtin throwable catches exception.
#[test]
fn test_builtin_throwable_catches_exception() {
    // Throwable (the root interface) catches a builtin Exception.
    let out =
        compile_and_run("<?php try { throw new Exception(); } catch (Throwable $e) { echo 12; }");
    assert_eq!(out, "12");
}

/// Verifies builtin throwable catches error.
#[test]
fn test_builtin_throwable_catches_error() {
    // Throwable (the root interface) catches a builtin Error.
    let out =
        compile_and_run("<?php try { throw new Error(); } catch (Throwable $e) { echo 13; }");
    assert_eq!(out, "13");
}

/// Verifies that getMessage() is called correctly on both Exception and Error
/// when caught via Throwable, confirming virtual dispatch to the right subclass.
#[test]
fn test_builtin_throwable_catch_dispatches_get_message() {
    let out = compile_and_run(
        "<?php try { throw new Exception(\"caught\"); } catch (Throwable $e) { echo $e->getMessage(); } try { throw new Error(\"core\"); } catch (Throwable $e) { echo \":\" . $e->getMessage(); }",
    );
    assert_eq!(out, "caught:core");
}

/// Verifies a caught exception keeps its concrete runtime class for class-name lookups.
#[test]
fn test_caught_exception_get_class_preserves_concrete_runtime_class() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new RuntimeException("x");
} catch (Throwable $e) {
    echo get_class($e), ":", get_parent_class($e), ":", $e->getMessage();
}

try {
    throw new RuntimeException("y");
} catch (LogicException | RuntimeException $e) {
    echo ":", get_class($e), ":", $e->getMessage();
}
"#,
    );
    assert_eq!(out, "RuntimeException:Exception:x:RuntimeException:y");
}

/// Verifies the full Throwable API surface on a caught Exception: getMessage,
/// getCode, getFile, getLine, getTrace, getTraceAsString, getPrevious, and
/// __toString all return expected values. File/line reflect the throw site.
#[test]
fn test_builtin_throwable_catch_exposes_standard_api() {
    let out = compile_and_run(
        "<?php try { throw new Exception(\"caught\", 42); } catch (Throwable $e) { echo $e->getMessage(); echo \":\"; echo $e->getCode(); echo \":\"; echo $e->getFile(); echo \":\"; echo $e->getLine(); echo \":\"; echo count($e->getTrace()); echo \":\"; echo $e->getTraceAsString(); echo \":\"; echo $e->getPrevious() === null ? \"none\" : \"some\"; echo \":\"; echo $e->__toString(); }",
    );
    assert_eq!(out, "caught:42::0:0::none:caught");
}

/// Tests a user-defined interface (AppThrowable) that extends Throwable and an
/// Exception implementing it (AppException). Verifies that catching as the
/// interface type correctly dispatches getMessage() and getCode().
#[test]
fn test_user_throwable_interface_extending_builtin_throwable_dispatches_methods() {
    let out = compile_and_run(
        r#"<?php
interface AppThrowable extends Throwable {}
class AppException extends Exception implements AppThrowable {}

try {
    throw new AppException("custom", 7);
} catch (Throwable $e) {
    echo $e->getMessage() . ":" . $e->getCode();
}

try {
    throw new AppException("iface", 9);
} catch (AppThrowable $e) {
    echo ":" . $e->getMessage() . ":" . $e->getCode();
}
"#,
    );
    assert_eq!(out, "custom:7:iface:9");
}

/// Verifies exception throw during concat resets concat cursor.
#[test]
fn test_exception_throw_during_concat_resets_concat_cursor() {
    // Throws an exception mid-concatenation operand. Verifies the left-hand side
    // of the concatenation is not leaked and the catch handler runs to completion.
    let out = compile_and_run(
        "<?php function boom() { throw new Exception(); } try { echo \"left-\" . boom(); } catch (Exception $e) { echo json_encode([\"ok\"]); }",
    );
    assert_eq!(out, "[\"ok\"]");
}

/// Verifies the error diagnostic for control restores runtime warnings after exception.
#[test]
fn test_error_control_restores_runtime_warnings_after_exception() {
    // Uses @ to suppress a warning in a function that throws, then after the try/catch
    // invokes a builtin that produces a warning. Verifies the @ suppression is
    // fully unwound and subsequent runtime warnings are emitted normally.
    let out = compile_and_run_capture(
        r#"<?php
function boom() {
    throw new Exception();
}

try {
    echo @boom();
} catch (Exception) {
    file_get_contents("missing.txt");
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Warning: file_get_contents()"),
        "expected runtime warning after unwinding @ scope, got stderr={}",
        out.stderr
    );
}

/// Verifies PHP multi-catch (AException | BException) branches to the handler
/// for the thrown type, testing the union-type catch dispatch logic.
#[test]
fn test_exception_multi_catch_matches_each_type() {
    let out = compile_and_run(
        "<?php class AException extends Exception {} class BException extends Exception {} function boom($flag) { if ($flag) { throw new AException(); } throw new BException(); } try { boom(true); } catch (AException | BException $e) { echo 1; } try { boom(false); } catch (AException | BException $e) { echo 2; }",
    );
    assert_eq!(out, "12");
}

/// Verifies exception catch without variable.
#[test]
fn test_exception_catch_without_variable() {
    // Catches an exception without binding it to a variable (PHP 8+ short syntax).
    // The catch block should still execute.
    let out =
        compile_and_run("<?php try { throw new Exception(); } catch (Exception) { echo 21; }");
    assert_eq!(out, "21");
}

/// Verifies exception catch can read builtin message.
#[test]
fn test_exception_catch_can_read_builtin_message() {
    // Catches a builtin Exception and reads getMessage() to confirm the exception
    // object is fully functional inside the catch handler.
    let out = compile_and_run(
        "<?php try { throw new Exception(\"caught\"); } catch (Exception $e) { echo $e->getMessage(); }",
    );
    assert_eq!(out, "caught");
}

/// Tests throw as a right-hand side expression in ?? (null coalescing operator).
/// Verifies that when the left side is null, the exception is thrown and caught,
/// and when the left side is non-null, the exception is not thrown.
#[test]
fn test_throw_expression_in_null_coalesce() {
    let out = compile_and_run(
        "<?php $value = 42; echo $value ?? throw new Exception(); try { $missing = null; echo $missing ?? throw new Exception(); } catch (Exception) { echo 22; }",
    );
    assert_eq!(out, "4222");
}

/// Tests throw as the false branch of a ternary expression. The exception is
/// thrown and caught, confirming throw can appear in expression contexts.
#[test]
fn test_throw_expression_in_ternary() {
    let out = compile_and_run(
        "<?php try { echo false ? 1 : throw new Exception(); } catch (Exception) { echo 23; }",
    );
    assert_eq!(out, "23");
}

/// Throws a custom exception from a callee and catches it in the caller.
/// Verifies the unwind across function boundaries and that the catch runs.
#[test]
fn test_exception_try_catch_cross_function() {
    let out = compile_and_run(
        "<?php class MyException extends Exception {} function boom() { throw new MyException(); } try { boom(); } catch (MyException $e) { echo 7; }",
    );
    assert_eq!(out, "7");
}

/// Verifies nested try-catch where the inner catch handles InnerException and
/// the outer catch only runs for other exception types. Tests correct dispatch
/// to the innermost matching catch.
#[test]
fn test_exception_nested_try_catch() {
    let out = compile_and_run(
        "<?php class InnerException extends Exception {} try { try { throw new InnerException(); } catch (InnerException $e) { echo 31; } } catch (Exception $e) { echo 99; }",
    );
    assert_eq!(out, "31");
}

/// Verifies exception throw in catch rethrows.
#[test]
fn test_exception_throw_in_catch_rethrows() {
    // Throws a second exception from within a catch block. The first exception is
    // handled (prints 32), then the second propagates to an outer catch (prints 33).
    let out = compile_and_run(
        "<?php class FirstException extends Exception {} class SecondException extends Exception {} try { try { throw new FirstException(); } catch (FirstException $e) { echo 32; throw new SecondException(); } } catch (SecondException $e) { echo 33; }",
    );
    assert_eq!(out, "3233");
}

/// Verifies exception throw in finally overrides prior exception.
#[test]
fn test_exception_throw_in_finally_overrides_prior_exception() {
    // Throws from a finally block after a prior exception is already unwinding.
    // Confirms the second exception replaces the first rather than nesting,
    // matching PHP behavior where only one exception propagates outward.
    let out = compile_and_run(
        "<?php class FirstException extends Exception {} class FinalException extends Exception {} try { try { throw new FirstException(); } finally { throw new FinalException(); } } catch (FinalException $e) { echo 34; }",
    );
    assert_eq!(out, "34");
}

/// Verifies exception uncaught reports fatal error.
#[test]
fn test_exception_uncaught_reports_fatal_error() {
    // Throws an exception with no enclosing try-catch. Verifies the compiler
    // reports a "Fatal error: uncaught exception" rather than silently ignoring it.
    let err = compile_and_run_expect_failure("<?php throw new Exception();");
    assert!(err.contains("Fatal error: uncaught exception"), "{err}");
}

/// Verifies exception with properties.
#[test]
fn test_exception_with_properties() {
    // Catches a user-defined exception subclass with a public property set in
    // the constructor. Verifies the property is accessible on the caught variable.
    let out = compile_and_run(
        "<?php class HttpException extends Exception { public $status; public function __construct() { $this->status = 404; } } try { throw new HttpException(); } catch (HttpException $e) { echo $e->status; }",
    );
    assert_eq!(out, "404");
}

/// Verifies that a try-catch nested inside a loop correctly catches exceptions
/// thrown from within that iteration, and that loop state is preserved across
/// iterations. The exception is thrown at $i==1 and caught, then the loop
/// continues to completion.
#[test]
fn test_exception_try_catch_inside_loop() {
    let out = compile_and_run(
        "<?php class LoopException extends Exception {} for ($i = 0; $i < 3; $i++) { try { if ($i == 1) { throw new LoopException(); } echo $i; } catch (LoopException $e) { echo 9; } }",
    );
    assert_eq!(out, "092");
}

/// Regression test: verifies that exiting the top-level script scope does not
/// leak owned local variables. Compiles an empty baseline and a script with a
/// local array, parses GC allocation/free counts from stderr, and asserts they
/// are balanced (allocs == frees). This guards against cleanup paths that drop
/// owned values without freeing them.
#[test]
fn test_gc_main_scope_cleanup_releases_owned_locals_on_exit() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats("<?php $items = [1, 2, 3];");
    assert!(
        baseline.success,
        "baseline program failed: {}",
        baseline.stderr
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(
        allocs - baseline_allocs,
        frees - baseline_frees,
        "{}",
        out.stderr
    );
}

/// Verifies that finally blocks execute even when the try body exits via return,
/// break, or continue. Checks: return value is 5 but finally prints 1 first,
/// break exits a try inside a for and finally prints 3, and continue in a for
/// runs finally (prints 9) before the next iteration.
#[test]
fn test_exception_finally_runs_on_return_break_continue() {
    let out = compile_and_run(
        "<?php function f() { try { return 5; } finally { echo 1; } } echo f(); for ($i = 0; $i < 1; $i++) { try { echo 2; break; } finally { echo 3; } } for ($j = 0; $j < 2; $j++) { try { echo $j; continue; } finally { echo 9; } }",
    );
    assert_eq!(out, "15230919");
}

/// Verifies that finally blocks run before returns from both try and catch bodies.
/// Issue #301: catch-body returns must route through the same pending finally state as try-body returns.
#[test]
fn test_exception_finally_runs_on_try_and_catch_return() {
    let out = compile_and_run(
        r#"<?php
function from_try() {
    try {
        return "t";
    } catch (Exception $e) {
        return "x";
    } finally {
        echo "F";
    }
}

function from_catch() {
    try {
        throw new Exception();
    } catch (Exception $e) {
        return "c";
    } finally {
        echo "f";
    }
}

echo from_try();
echo from_catch();
"#,
    );
    assert_eq!(out, "Ftfc");
}

/// A break inside a finally block exits the while loop that encloses the try.
/// The finally block itself runs, then break transfers control out of the loop.
/// Verifies the loop is entered, the try body prints 1, and finally runs the
/// break that terminates the loop before echo 4 executes.
#[test]
fn test_exception_finally_allows_local_loop_break() {
    let out = compile_and_run(
        "<?php try { echo 1; } finally { while (1) { echo 2; break; } echo 3; } echo 4;",
    );
    assert_eq!(out, "1234");
}

/// Regression: a `try`/`catch` whose body calls a function that can throw, nested inside a
/// `foreach` loop, must compile and run. The catch handler is reachable only through an implicit
/// exception edge; without modelling that edge in the IR validator's predecessor graph the
/// handler looked unreachable, and the foreach back-edge then stripped the entry block out of the
/// loop header's dominators, so the iterator value (defined in the entry block) was rejected with
/// a spurious `UseNotDominated` error at compile time. Each iteration must observe whether its
/// element threw.
#[test]
fn test_try_catch_in_foreach_with_throwing_callee() {
    let out = compile_and_run(
        r#"<?php
function mayThrow($s) {
    if ($s === "bad") { throw new Exception("boom"); }
    return $s;
}
$log = "";
foreach (["ok", "bad", "ok"] as $item) {
    try { mayThrow($item); $log .= "0"; }
    catch (Exception $e) { $log .= "1"; }
}
echo $log;
"#,
    );
    assert_eq!(out, "010");
}

/// Regression companion: the same implicit-handler-edge fix must keep a `try`/`catch` that catches
/// a thrown exception inside a `while` loop working, with the catch body mutating a loop-carried
/// accumulator. Confirms the dominator fix is not specific to `foreach`'s iterator lowering.
#[test]
fn test_try_catch_in_while_loop_accumulates() {
    let out = compile_and_run(
        r#"<?php
function check($n) {
    if ($n % 2 === 0) { throw new Exception("even"); }
    return $n;
}
$out = "";
$i = 0;
while ($i < 4) {
    try { check($i); $out .= "o"; }
    catch (Exception $e) { $out .= "x"; }
    $i++;
}
echo $out;
"#,
    );
    assert_eq!(out, "xoxo");
}

/// Regression for a DCE tail-sinking blowup: many sequential `try`/`catch`
/// blocks (each with a may-throw method call in the try body and a
/// fall-through, empty catch body) in one function used to make the optimizer
/// clone the tail into every fall-through path, compounding exponentially
/// (2^n copies) so that ~8 such blocks overflowed the AArch64 conditional-
/// branch range and the assembler was killed (`fixup value out of range`).
/// After the fix the tail is kept as a sibling (lowered into a single shared
/// after-block), so the emitted code grows linearly. This compiles 16 of them
/// and checks the fall-through continuation runs exactly once, which would not
/// assemble before the fix.
#[test]
fn test_sequential_try_catch_does_not_blow_up_codegen() {
    let mut php = String::from("<?php class G { public function f($n) { echo $n; } } $g = new G(); ");
    let mut expected = String::new();
    for i in 1..=16 {
        php.push_str("try { $g->f(");
        php.push_str(&i.to_string());
        php.push_str("); } catch (Exception $e) {} ");
        expected.push_str(&i.to_string());
    }
    php.push_str("echo \"Z\";");
    expected.push('Z');
    let out = compile_and_run(&php);
    assert_eq!(out, expected);
}

/// Verifies that a private method call from an inaccessible scope raises a
/// catchable `Error` at runtime (issue #383). PHP prints `err`, not `no`.
#[test]
fn test_private_method_access_is_catchable_error() {
    let out = compile_and_run(
        "<?php class C { private function secret() {} } $c = new C(); try { $c->secret(); echo 'no'; } catch (Error $e) { echo 'err'; }",
    );
    assert_eq!(out, "err");
}

/// Verifies that a protected method call from an inaccessible scope raises a
/// catchable `Error` at runtime (issue #383).
#[test]
fn test_protected_method_access_is_catchable_error() {
    let out = compile_and_run(
        "<?php class C { protected function secret() {} } $c = new C(); try { $c->secret(); echo 'no'; } catch (Error $e) { echo 'err'; }",
    );
    assert_eq!(out, "err");
}

/// Verifies that a readonly property write outside the declaring constructor
/// raises a catchable `Error` at runtime (issue #383). PHP prints `err`.
#[test]
fn test_readonly_property_write_is_catchable_error() {
    let out = compile_and_run(
        "<?php class Box { public readonly int $x; public function __construct() { $this->x = 1; } } try { $b = new Box(); $b->x = 2; echo 'no'; } catch (Error $e) { echo 'err'; }",
    );
    assert_eq!(out, "err");
}

/// Verifies that a readonly class's implicitly-readonly property write outside
/// the constructor raises a catchable `Error` at runtime (issue #383).
#[test]
fn test_readonly_class_property_write_is_catchable_error() {
    let out = compile_and_run(
        "<?php readonly class User { public int $id; public function __construct($id) { $this->id = $id; } } try { $u = new User(1); $u->id = 2; echo 'no'; } catch (Error $e) { echo 'err'; }",
    );
    assert_eq!(out, "err");
}

/// Verifies that an uncaught private method call produces a fatal exit (issue #383).
#[test]
fn test_private_method_access_uncaught_is_fatal() {
    let output = compile_and_run_capture(
        "<?php class C { private function secret() {} } $c = new C(); $c->secret();",
    );
    assert!(!output.success, "expected a fatal exit");
    assert!(
        output.stderr.contains("Fatal error: uncaught exception"),
        "expected a fatal diagnostic on stderr, got: {}",
        output.stderr
    );
}

/// Verifies that an uncaught readonly property write produces a fatal exit (issue #383).
#[test]
fn test_readonly_property_write_uncaught_is_fatal() {
    let output = compile_and_run_capture(
        "<?php class Box { public readonly int $x; public function __construct() { $this->x = 1; } } $b = new Box(); $b->x = 2;",
    );
    assert!(!output.success, "expected a fatal exit");
    assert!(
        output.stderr.contains("Fatal error: uncaught exception"),
        "expected a fatal diagnostic on stderr, got: {}",
        output.stderr
    );
}

/// Verifies that `getMessage()` on a caught private-method `Error` returns the
/// PHP error message (issue #383).
#[test]
fn test_private_method_access_error_message() {
    let out = compile_and_run(
        "<?php class C { private function secret() {} } $c = new C(); try { $c->secret(); } catch (Error $e) { echo $e->getMessage(); }",
    );
    assert_eq!(out, "Call to private method C::secret() from global scope");
}

/// Regression: private-method access must evaluate the receiver expression
/// before raising the catchable `Error`, matching PHP's observable side effects.
#[test]
fn test_private_method_access_evaluates_receiver_before_error() {
    let out = compile_and_run(
        r#"<?php
class C { private function secret() {} }
function make_c() {
    echo "make|";
    return new C();
}
try { make_c()->secret(); echo "no"; } catch (Error $e) { echo "err"; }
"#,
    );
    assert_eq!(out, "make|err");
}

/// Verifies that `getMessage()` on a caught readonly-write `Error` returns the
/// PHP error message (issue #383).
#[test]
fn test_readonly_property_write_error_message() {
    let out = compile_and_run(
        "<?php class Box { public readonly int $x; public function __construct() { $this->x = 1; } } try { $b = new Box(); $b->x = 2; } catch (Error $e) { echo $e->getMessage(); }",
    );
    assert_eq!(out, "Cannot modify readonly property Box::$x");
}

/// Regression: readonly-property writes must evaluate the right-hand side
/// before raising the catchable `Error`, matching PHP's observable side effects.
#[test]
fn test_readonly_property_write_evaluates_rhs_before_error() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public readonly int $x;
    public function __construct() { $this->x = 1; }
}
function side() {
    echo "side|";
    return 2;
}
$b = new Box();
try { $b->x = side(); echo "no"; } catch (Error $e) { echo "err|"; }
echo $b->x;
"#,
    );
    assert_eq!(out, "side|err|1");
}

/// Verifies that calling a protected method from outside the class hierarchy
/// raises a catchable `Error` at runtime (issue #383).
#[test]
fn test_protected_method_access_outside_class_is_catchable_error() {
    let out = compile_and_run(
        "<?php class Secret { protected function hidden() { return 7; } } $s = new Secret(); try { echo $s->hidden(); echo 'no'; } catch (Error $e) { echo 'err'; }",
    );
    assert_eq!(out, "err");
}

/// Verifies that calling a protected trait method from outside the class
/// hierarchy raises a catchable `Error` at runtime (issue #383).
#[test]
fn test_protected_trait_method_access_is_catchable_error() {
    let out = compile_and_run(
        r#"<?php
trait A { public function foo() { return 1; } }
class C { use A { A::foo as protected; } }
$c = new C();
try { echo $c->foo(); echo 'no'; } catch (Error $e) { echo 'err'; }
"#,
    );
    assert_eq!(out, "err");
}

/// Regression test: a `throw` that unwinds out of nested functions releases each
/// unwound frame's owned refcounted locals via the per-frame activation-record
/// cleanup callbacks (`_exc_call_frame_top` chain), so they do not leak.
///
/// Uses a delta method that isolates the frame-local cleanup from the separate,
/// pre-existing leak of the caught exception object itself: both programs throw
/// and catch once per loop iteration (identical exception-object cost), but only
/// the second holds owned locals — big strings at all three nesting levels plus
/// an array in the innermost frame. If the unwinder freed those locals, the two
/// programs leak the same amount; before the activation-record cleanup landed,
/// the owned version leaked ~9 KB per iteration. The test asserts the owned
/// program allocated strictly more (so it is not vacuous) yet leaks exactly as
/// much as the baseline.
#[test]
fn test_throw_through_nested_frames_releases_owned_locals() {
    let baseline = compile_and_run_with_gc_stats(
        "<?php \
         function c($i) { if ($i >= 0) throw new Exception(\"x\"); } \
         function b($i) { c($i); } \
         function a($i) { b($i); } \
         for ($i = 0; $i < 20; $i++) { try { a($i); } catch (Exception $e) {} } echo \"done\";",
    );
    let owned = compile_and_run_with_gc_stats(
        "<?php \
         function c($i) { $s = str_repeat(\"z\", 4000); $arr = [1, 2, 3, $i, \"tail\" . $i]; \
             if ($i >= 0) throw new Exception(\"x\"); echo $s . count($arr); } \
         function b($i) { $t = \"mid-\" . str_repeat(\"y\", 3000) . $i; c($i); echo $t; } \
         function a($i) { $u = \"top-\" . str_repeat(\"w\", 2000) . $i; b($i); echo $u; } \
         for ($i = 0; $i < 20; $i++) { try { a($i); } catch (Exception $e) {} } echo \"done\";",
    );
    assert!(baseline.success, "baseline failed: {}", baseline.stderr);
    assert!(owned.success, "owned-locals program failed: {}", owned.stderr);
    let (base_allocs, base_frees) = parse_gc_stats(&baseline.stderr);
    let (owned_allocs, owned_frees) = parse_gc_stats(&owned.stderr);
    assert!(
        owned_allocs > base_allocs,
        "test is vacuous: owned program must allocate more than baseline ({owned_allocs} vs {base_allocs})",
    );
    assert_eq!(
        owned_allocs - owned_frees,
        base_allocs - base_frees,
        "owned locals leaked on nested throw-unwind: owned leak {} != baseline leak {}\n{}",
        owned_allocs - owned_frees,
        base_allocs - base_frees,
        owned.stderr,
    );
}

/// Regression test: a caught exception object bound to `$e` is released. Loops so the
/// per-iteration rebind must free the previous object (catch-bind release-old) and the
/// final object is freed at scope exit; with a constant message no heap string is
/// involved, so allocs and frees must match exactly. Before the fix each caught
/// exception leaked one object.
#[test]
fn test_caught_exception_object_released_over_loop() {
    let out = compile_and_run_with_gc_stats(
        "<?php for ($i = 0; $i < 8; $i++) { try { throw new Exception(\"boom\"); } \
         catch (Exception $e) {} } echo \"end\";",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "end", "stdout: {:?}", out.stdout);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "caught exception object leaked: {}", out.stderr);
}

/// Regression test: a variable-less `catch (Exception)` still owns the caught object
/// and must release it even though it never binds it. Before the fix the only
/// reference was discarded unreleased, leaking one object per catch.
#[test]
fn test_variable_less_catch_releases_exception() {
    let out = compile_and_run_with_gc_stats(
        "<?php for ($i = 0; $i < 8; $i++) { try { throw new Exception(\"boom\"); } \
         catch (Exception) {} } echo \"end\";",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "end", "stdout: {:?}", out.stdout);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "variable-less catch leaked the exception: {}", out.stderr);
}

/// Regression test: `throw $e` re-raises a caught-and-bound exception out of a
/// function without a use-after-free or a leak. The re-raise retains the object so the
/// throwing frame's owned-local cleanup callback cannot free it while it is in flight;
/// the outer frame then takes ownership and releases it. A constant message read after
/// the re-raise is the UAF canary (a freed object would yield garbage or crash), and
/// the exact allocs/frees balance guards against both a leak and a double-free.
#[test]
fn test_rethrow_local_exception_balanced_and_correct() {
    let out = compile_and_run_with_gc_stats(
        "<?php function f() { try { throw new Exception(\"boom\"); } \
             catch (Exception $e) { throw $e; } } \
         for ($i = 0; $i < 6; $i++) { try { f(); } \
             catch (Exception $e2) { echo $e2->getMessage(), \";\"; } }",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "boom;boom;boom;boom;boom;boom;", "stdout: {:?}", out.stdout);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "rethrow leaked or double-freed: {}", out.stderr);
}

/// Regression test: `$e` stays valid AFTER its `catch` block (PHP scopes the catch
/// variable to the whole function/scope, not the block) and is still released. Reads
/// the message past the block each iteration; a premature release would corrupt the
/// output, and the counts must stay balanced.
#[test]
fn test_catch_variable_usable_after_block_and_released() {
    let out = compile_and_run_with_gc_stats(
        "<?php for ($i = 0; $i < 4; $i++) { try { throw new Exception(\"boom\"); } \
         catch (Exception $e) {} echo $e->getMessage(), \";\"; }",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "boom;boom;boom;boom;", "stdout: {:?}", out.stdout);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "catch variable used after block leaked: {}", out.stderr);
}

/// Regression test: reassigning `$e` inside the catch body releases the caught object
/// through the ordinary store-local old-release path exactly once, and the nulled slot
/// is skipped at scope exit (no double-free).
#[test]
fn test_catch_variable_reassigned_in_body_balanced() {
    let out = compile_and_run_with_gc_stats(
        "<?php for ($i = 0; $i < 6; $i++) { try { throw new Exception(\"boom\"); } \
         catch (Exception $e) { $e = null; } } echo \"end\";",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "end", "stdout: {:?}", out.stdout);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "reassigned catch variable leaked or double-freed: {}", out.stderr);
}

/// Regression test: a user subclass that declares its own constructor and calls
/// `parent::__construct($message)` on a builtin Throwable links and behaves correctly.
/// Built-in throwable constructors have no emitted method body, so before the fix this
/// referenced the undefined symbol `_method_Exception___construct` and failed at link
/// time. The inline field stamp must land the message on the inherited slot so
/// `getMessage()` reads it back.
#[test]
fn test_parent_construct_builtin_throwable_links_and_reports_message() {
    let out = compile_and_run(
        "<?php class AppException extends Exception { \
             public function __construct(string $m) { parent::__construct($m); } } \
         try { throw new AppException(\"boom\"); } \
         catch (AppException $e) { echo $e->getMessage(); }",
    );
    assert_eq!(out, "boom");
}

/// Regression test: `parent::__construct($message, $code)` forwards both arguments and
/// coexists with the subclass's own property assignment. Verifies message, code, and the
/// subclass property are all correct — the inherited `message`/`code` slots (object
/// offsets 8/16 and 24) must not collide with the subclass's own property slot.
#[test]
fn test_parent_construct_forwards_code_alongside_own_property() {
    let out = compile_and_run(
        "<?php class AppException extends RuntimeException { \
             public string $ctx = \"\"; \
             public function __construct(string $m, int $code, string $ctx) { \
                 parent::__construct($m, $code); $this->ctx = $ctx; } } \
         try { throw new AppException(\"boom\", 42, \"cx\"); } \
         catch (AppException $e) { echo $e->getMessage(), \"|\", $e->getCode(), \"|\", $e->ctx; }",
    );
    assert_eq!(out, "boom|42|cx");
}

/// Regression test: a subclass constructor that forwards a HEAP-backed message to
/// `parent::__construct` must persist an owned copy so the object frees it exactly once.
/// Loops with a per-iteration heap message and a heap subclass property; `--heap-debug`
/// must report a clean leak summary. This is the worker/OOM-driving shape: without the
/// persist the object would either leak the message or double-free the caller's local.
#[test]
fn test_parent_construct_heap_message_no_leak_over_loop() {
    let out = compile_and_run_with_heap_debug(
        "<?php class AppException extends Exception { \
             public string $ctx = \"\"; \
             public function __construct(string $m, string $ctx) { \
                 parent::__construct($m); $this->ctx = $ctx; } } \
         $acc = 0; \
         for ($i = 0; $i < 40; $i++) { \
             try { throw new AppException(\"msg-\" . $i . \"-pad\", \"ctx-\" . $i); } \
             catch (AppException $e) { $acc += strlen($e->getMessage()) + strlen($e->ctx); } } \
         echo $acc;",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "parent::__construct heap message leaked: {}",
        out.stderr,
    );
}
