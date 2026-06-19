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
