//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of exceptions, including exception try catch same function, builtin exception try catch, and builtin exception message api.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use crate::support::*;

/// Verifies a Throwable owns a dynamically concatenated message after the source
/// temporary is released and unrelated catch-block strings reuse scratch storage.
#[test]
fn test_dynamic_exception_message_is_persisted() {
    let out = compile_and_run(
        r#"<?php
$suffix = "payload";
try {
    throw new Error("dynamic:" . $suffix);
} catch (Throwable $e) {
    echo get_class($e) . "|" . $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Error|dynamic:payload");
}

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

/// Verifies `getMessage()` returns a caller-owned string without consuming the
/// builtin Throwable payload used by later reads and `__toString()`.
#[test]
fn test_builtin_exception_get_message_does_not_consume_payload() {
    let out = compile_and_run(
        "<?php $e = new Exception(\"boom\"); echo $e->getMessage(); echo \":\"; echo $e->getMessage(); echo \":\"; echo $e->__toString();",
    );
    assert_eq!(out, "boom:boom:boom");
}

/// Checks that Exception messages built from temporary string results survive the throw.
#[test]
fn test_builtin_exception_message_persists_concatenated_temporary() {
    let out = compile_and_run(
        r#"<?php
$name = "dynamic";
try {
    throw new Exception($name . " boom");
} catch (Exception $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "dynamic boom");
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
    let out = compile_and_run("<?php try { throw new Error(); } catch (Throwable $e) { echo 13; }");
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
/// __toString all return expected values.
///
/// `getFile()` is compared against `__FILE__` rather than to a literal: both resolve through the
/// same canonicalization, and the test's script lives in a temp directory whose name changes on
/// every run. `getLine()` is `1` because the whole probe is one line — and it is the line of the
/// `new`, which is what PHP records; the two coincide here only because they are the same line.
///
/// `getTrace()`/`getTraceAsString()` stay empty: elephc keeps no call stack to render, where
/// reference PHP would report `#0 {main}`.
#[test]
fn test_builtin_throwable_catch_exposes_standard_api() {
    let out = compile_and_run(
        "<?php try { throw new Exception(\"caught\", 42); } catch (Throwable $e) { echo $e->getMessage(); echo \":\"; echo $e->getCode(); echo \":\"; echo $e->getFile() === __FILE__ ? \"file\" : \"BAD(\" . $e->getFile() . \")\"; echo \":\"; echo $e->getLine(); echo \":\"; echo count($e->getTrace()); echo \":\"; echo $e->getTraceAsString(); echo \":\"; echo $e->getPrevious() === null ? \"none\" : \"some\"; echo \":\"; echo $e->__toString(); }",
    );
    assert_eq!(out, "caught:42:file:1:0::none:caught");
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
    // Throws an exception with no enclosing try-catch. The diagnostic names the CLASS, and
    // carries no `": "` because the message is empty — reference PHP 8.5.6 prints exactly
    // `Fatal error: Uncaught Exception in <file>:<line>` here, and elephc emits everything up
    // to that suffix (there is no throw-site origin to report; see issue #660).
    let err = compile_and_run_expect_failure("<?php throw new Exception();");
    assert!(err.contains("Fatal error: Uncaught Exception"), "{err}");
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
    // Byte-identical to reference PHP 8.5.6 up to its ` in <file>:<line>` suffix.
    // STDOUT: PHP writes its uncaught report there, and elephc now matches — measured by capturing
    // the two streams into separate files, where stderr came back empty.
    assert_eq!(output.exit_code, Some(255));
    assert!(
        output
            .stdout
            .contains("Fatal error: Uncaught Error: Call to private method C::secret() from global scope"),
        "expected a fatal diagnostic naming the class and message, got: {}",
        output.stdout
    );
}

/// Verifies that an uncaught readonly property write produces a fatal exit (issue #383).
#[test]
fn test_readonly_property_write_uncaught_is_fatal() {
    let output = compile_and_run_capture(
        "<?php class Box { public readonly int $x; public function __construct() { $this->x = 1; } } $b = new Box(); $b->x = 2;",
    );
    assert!(!output.success, "expected a fatal exit");
    // Byte-identical to reference PHP 8.5.6 up to its ` in <file>:<line>` suffix.
    // See above: the report is on stdout.
    assert_eq!(output.exit_code, Some(255));
    assert!(
        output
            .stdout
            .contains("Fatal error: Uncaught Error: Cannot modify readonly property Box::$x"),
        "expected a fatal diagnostic naming the class and message, got: {}",
        output.stdout
    );
}

/// Verifies an explicit `exit(1)` keeps its caller-selected status after fatal exits move to 255.
#[test]
fn test_explicit_exit_one_preserves_status() {
    let output = compile_and_run_capture("<?php exit(1);");
    assert!(!output.success, "expected an explicit non-zero exit");
    assert_eq!(output.exit_code, Some(1));
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

/// An object-returning function whose `try` and every `catch` terminate must leave the dead
/// `try.after` join unreachable instead of synthesizing a typed fall-through return.
#[test]
fn test_object_return_dead_try_after_is_unreachable() {
    let out = compile_and_run(
        r#"<?php
final class Conn { public function __construct(public string $dsn) {} }
final class Factory {
    public function create(string $dsn): Conn {
        try { return new Conn($dsn); }
        catch (\Throwable $e) { throw new \RuntimeException('fail'); }
    }
}
echo (new Factory())->create('pg')->dsn;
"#,
    );
    assert_eq!(out, "pg");
}

/// An array-returning function with `finally` must also mark `try.after` unreachable when neither
/// the `try` nor any `catch` can fall through the finalizer into the join.
#[test]
fn test_array_return_dead_try_after_with_finally_is_unreachable() {
    let out = compile_and_run(
        r#"<?php
function values(): array {
    try { return [1, 2]; }
    catch (\Throwable $e) { throw new \RuntimeException('fail'); }
    finally { $cleanup = true; }
}
$values = values();
echo $values[0] . ',' . $values[1];
"#,
    );
    assert_eq!(out, "1,2");
}

/// The builtin exception constructors accept PHP's third `$previous` parameter (positional or
/// the `previous:` named argument), store it on the Throwable payload, and expose it through
/// `getPrevious()`. Byte-parity vs PHP 8.5 for message/code/`getPrevious()` round-trips.
#[test]
fn test_exception_constructor_accepts_previous() {
    let out = compile_and_run(
        "<?php function f(): string { try { try { throw new \\ValueError('inner'); } catch (\\ValueError $e) { throw new \\InvalidArgumentException('outer: ' . $e->getMessage(), $e->getCode(), previous: $e); } } catch (\\InvalidArgumentException $x) { $prev = $x->getPrevious(); return $x->getMessage() . '/' . $x->getCode() . '/' . ($prev === null ? 'none' : $prev->getMessage()); } } echo f();",
    );
    assert_eq!(out, "outer: inner/0/inner");
}

/// `getPrevious()` returns `?Throwable`; method calls on that nullable interface type must use
/// compact Throwable intrinsics (the interface vtable slots stay empty for builtins).
#[test]
fn test_nullable_throwable_get_message_via_previous() {
    let out = compile_and_run(
        "<?php function show(?Throwable $t): string { return $t === null ? 'null' : $t->getMessage(); } $inner = new ValueError('inner'); $outer = new Exception('outer', 0, $inner); echo show($outer->getPrevious());",
    );
    assert_eq!(out, "inner");
}

/// Verifies a `finally` replacement appends the pending Throwable after an explicit chain.
#[test]
fn test_finally_replacement_appends_pending_exception_to_previous_chain() {
    let out = compile_and_run(
        r#"<?php
$explicit = new Exception("explicit");
try {
    try { throw new Error("old"); }
    finally { throw new RuntimeException("new", 0, $explicit); }
} catch (Throwable $e) {
    $explicitPrevious = $e->getPrevious();
    $pendingPrevious = $explicitPrevious->getPrevious();
    echo get_class($e), ":", $e->getMessage(), "|";
    echo get_class($explicitPrevious), ":", $explicitPrevious->getMessage(), "|";
    echo get_class($pendingPrevious), ":", $pendingPrevious->getMessage(), "|";
    echo $pendingPrevious->getPrevious() === null ? "end" : "more";
}
"#,
    );
    assert_eq!(out, "RuntimeException:new|Exception:explicit|Error:old|end");
}

/// Verifies implicit chaining does not duplicate an explicitly supplied pending Throwable.
#[test]
fn test_finally_replacement_does_not_cycle_existing_previous_exception() {
    let out = compile_and_run(
        r#"<?php
$old = new Error("old");
try {
    try { throw $old; }
    finally { throw new Exception("new", 0, $old); }
} catch (Throwable $e) {
    echo $e->getPrevious() === $old ? "same" : "other";
    echo "|";
    echo $e->getPrevious()->getPrevious() === null ? "end" : "cycle";
}
"#,
    );
    assert_eq!(out, "same|end");
}

/// Verifies exception edges preserve register-allocated values that remain live
/// into a catch block under enough pressure to require spills.
#[test]
fn test_exception_catch_preserves_live_values_after_register_allocation() {
    let out = compile_and_run(
        r#"<?php
function throw_under_pressure(int $value): int {
    if ($value > 0) { throw new Exception("expected"); }
    return $value;
}
function preserve_live_values(int $seed): int {
    $v01 = $seed + 1;  $v02 = $seed + 2;
    $v03 = $seed + 3;  $v04 = $seed + 4;
    $v05 = $seed + 5;  $v06 = $seed + 6;
    $v07 = $seed + 7;  $v08 = $seed + 8;
    $v09 = $seed + 9;  $v10 = $seed + 10;
    $v11 = $seed + 11; $v12 = $seed + 12;
    $v13 = $seed + 13; $v14 = $seed + 14;
    $v15 = $seed + 15; $v16 = $seed + 16;
    $v17 = $seed + 17; $v18 = $seed + 18;
    $v19 = $seed + 19; $v20 = $seed + 20;
    $caught = 0;
    try { throw_under_pressure($seed); }
    catch (Exception $error) { $caught = 1; }
    return $v01 + $v02 + $v03 + $v04 + $v05 + $v06 + $v07 + $v08 + $v09 + $v10
        + $v11 + $v12 + $v13 + $v14 + $v15 + $v16 + $v17 + $v18 + $v19 + $v20
        + $caught;
}
echo preserve_live_values($argc);
"#,
    );
    assert_eq!(out, "231");
}

// --- PHP 8 arithmetic errors (`DivisionByZeroError` / `ArithmeticError`) ---

/// Verifies `%` by zero throws a catchable `DivisionByZeroError` with php-src's wording.
///
/// elephc used to return `0`, so no `catch` clause could ever observe the error. The divisor
/// comes from `$argc - 1` so the constant folders cannot evaluate it at compile time.
#[test]
fn test_modulo_by_zero_throws_division_by_zero_error() {
    let out = compile_and_run(
        r#"<?php
$z = $argc - 1;
try { echo 1 % $z; } catch (DivisionByZeroError $e) { echo get_class($e), ':', $e->getMessage(); }
echo '|';
$a = 7;
try { $a %= $z; } catch (DivisionByZeroError $e) { echo 'compound:', $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "DivisionByZeroError:Modulo by zero|compound:Modulo by zero"
    );
}

/// Verifies `/` by zero throws a catchable `DivisionByZeroError` for int and float operands.
///
/// PHP throws for `1/0`, `1.0/0`, `1/0.0`, `0/0`, and `-1.0/0.0` alike — the IEEE `INF`/`NaN`
/// result is only reachable through `fdiv()`. elephc used to hand back `INF`.
#[test]
fn test_division_by_zero_throws_for_int_and_float_operands() {
    let out = compile_and_run(
        r#"<?php
$z = $argc - 1;
$zf = 0.0 * $argc;
try { echo 1 / $z; } catch (DivisionByZeroError $e) { echo 'i:', $e->getMessage(); }
echo '|';
try { echo 1.0 / $zf; } catch (DivisionByZeroError $e) { echo 'f:', $e->getMessage(); }
echo '|';
try { echo 1 / $zf; } catch (DivisionByZeroError $e) { echo 'if:', $e->getMessage(); }
echo '|';
try { echo $zf / $zf; } catch (DivisionByZeroError $e) { echo 'ff:', $e->getMessage(); }
echo '|';
try { echo -1.0 / $zf; } catch (DivisionByZeroError $e) { echo 'nf:', $e->getMessage(); }
echo '|';
$b = 7;
try { $b /= $z; } catch (DivisionByZeroError $e) { echo 'compound:', $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "i:Division by zero|f:Division by zero|if:Division by zero|\
         ff:Division by zero|nf:Division by zero|compound:Division by zero"
    );
}

/// Verifies the arithmetic `DivisionByZeroError` is a real `ArithmeticError`/`Throwable`.
#[test]
fn test_division_by_zero_error_matches_parent_handlers() {
    let out = compile_and_run(
        r#"<?php
$z = $argc - 1;
try { echo 1 % $z; } catch (ArithmeticError $e) { echo 'arithmetic'; }
echo '|';
try { echo 1 / $z; } catch (Error $e) { echo 'error'; }
echo '|';
try { echo 1 % $z; } catch (Throwable $e) { echo get_class($e); }
echo '|';
try { echo intdiv(1, $z); } catch (DivisionByZeroError $e) { echo 'intdiv:', $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "arithmetic|error|DivisionByZeroError|intdiv:Division by zero"
    );
}

/// Verifies a negative shift count throws a catchable `ArithmeticError` for `<<` and `>>`.
///
/// The hardware shift masks the count, so `1 << -1` used to evaluate to `PHP_INT_MIN`.
#[test]
fn test_negative_shift_count_throws_arithmetic_error() {
    let out = compile_and_run(
        r#"<?php
$neg = -1 * $argc;
try { echo (1 * $argc) << $neg; } catch (ArithmeticError $e) { echo get_class($e), ':', $e->getMessage(); }
echo '|';
try { echo (1 * $argc) >> $neg; } catch (ArithmeticError $e) { echo 'shr:', $e->getMessage(); }
echo '|';
$c = 5;
try { $c <<= $neg; } catch (ArithmeticError $e) { echo 'compound:', $e->getMessage(); }
echo '|';
try { echo (1 * $argc) << $neg; } catch (Throwable $e) { echo get_class($e); }
"#,
    );
    assert_eq!(
        out,
        "ArithmeticError:Bit shift by negative number|\
         shr:Bit shift by negative number|\
         compound:Bit shift by negative number|ArithmeticError"
    );
}

/// Verifies non-zero divisors and non-negative shift counts keep working after the guards.
#[test]
fn test_arithmetic_guards_do_not_disturb_normal_operands() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
echo 7 % 3, '|', (7 * $n) % (3 * $n), '|', 8 / 2, '|', (7.5 * $n) / (2.5 * $n);
echo '|', intdiv(7, 2), '|', fdiv(1, 0), '|', (1 * $n) << (3 * $n), '|', (-8 * $n) >> (1 * $n);
"#,
    );
    assert_eq!(out, "1|1|4|3|3|INF|8|-4");
}

/// The caught exception shadows whatever its name held before the `try`.
///
/// A catch binds its variable from the handler's first statement. The constant
/// environment the handler is walked in is derived from the try body's writes,
/// so a body that never touches that name left the incoming value standing — and
/// the handler then read the Throwable as that value. Substituting a string for
/// an exception does not even reach a wrong answer: it stops the compile at
/// `method call receiver for PHP type Str`.
///
/// The exit-path simulation had always removed the binding. Two walks over the
/// same block that disagree about what is live in it can only be right in one.
#[test]
fn test_a_catch_binding_shadows_the_name_it_reuses() {
    let out = compile_and_run(
        r#"<?php
function shadowed(): string {
    $e = 'stale';
    try { throw new RuntimeException('fresh'); }
    catch (RuntimeException $e) { return $e->getMessage(); }
}
function untouched(): string {
    $keep = 'kept';
    try { throw new RuntimeException('x'); }
    catch (RuntimeException $e) { return $keep; }
}
echo shadowed(), '|', untouched();
"#,
    );
    // The second half is the other direction: a name the try body cannot write
    // and the catch does not bind still folds, so the fix stays a rule about the
    // BINDING rather than a blanket clear of the handler's environment.
    assert_eq!(out, "fresh|kept");
}

/// A value written inside a `try` survives into the `catch`.
///
/// It did not. Every write the try body made was discarded the moment the
/// exception was caught, and the catch — and everything after the statement —
/// read what the variable held BEFORE the `try`. Silently: no diagnostic, no
/// crash, just an answer PHP disagrees with, in one of the most ordinary shapes
/// there is (accumulate in a try, adjust in the catch).
///
/// Two independent causes, both about a catch being entered from the MIDDLE of
/// the try body: the AST constant propagation walked the catch in the
/// environment from before the `try`, and the exit environment merged that same
/// stale path out past the whole statement.
#[test]
fn test_a_write_inside_a_try_survives_the_catch() {
    let out = compile_and_run(
        r#"<?php
function accumulate(): int {
    $t = 0;
    try {
        $t += 5;
        throw new RuntimeException('x');
    } catch (RuntimeException $e) {
        $t += 7;
    }
    return $t;
}
function read_in_catch(): int {
    $t = 0;
    try { $t = 5; throw new RuntimeException('x'); }
    catch (RuntimeException $e) { return $t; }
    return -1;
}
function read_after(): string {
    $s = 'a';
    try { $s = 'bb'; throw new RuntimeException('x'); }
    catch (RuntimeException $e) { }
    return $s;
}
echo accumulate(), '|', read_in_catch(), '|', read_after();
"#,
    );
    assert_eq!(out, "12|5|bb");
}

/// A store made before a throwing CALL is not dead because a later store
/// overwrites it.
///
/// Dead-store elimination walks a CFG built from terminators, and a `may_throw`
/// instruction is not one — so the block that throws out of the middle of a
/// `try` looked like it reached only the block its `br` named, and `$t = 9`
/// looked like it overwrote `$t = 5` on the only path there was. Both earlier
/// stores were neutralized to `nop`, and the catch fell through to a load of a
/// slot nothing had written: the function returned a different value on every
/// run, being whatever the frame's stack happened to hold.
///
/// The string case is here because it was RIGHT while the int case was wrong —
/// refcounted locals are zero-initialized and scalars are not, so the same
/// defect surfaced as a stale value in one and as an address in the other.
#[test]
fn test_a_store_before_a_throwing_call_is_not_dead() {
    let out = compile_and_run(
        r#"<?php
function boom(): void { throw new RuntimeException('x'); }
function overwritten_after_the_call(): int {
    $t = 0;
    try { $t = 5; boom(); $t = 9; }
    catch (RuntimeException $e) { }
    return $t;
}
function overwritten_after_a_literal_throw(): int {
    $t = 0;
    try { $t = 5; $z = 1; throw new RuntimeException('x'); $t = 9; }
    catch (RuntimeException $e) { }
    return $t;
}
function strings_too(): string {
    $t = 'before';
    try { $t = 'inside'; boom(); $t = 'after'; }
    catch (RuntimeException $e) { }
    return $t;
}
echo overwritten_after_the_call(), '|', overwritten_after_a_literal_throw(), '|', strings_too();
"#,
    );
    assert_eq!(out, "5|5|inside");
}

/// The try body's stores stay dead when nothing in it can throw.
///
/// The guard that keeps the two above correct must not become "never eliminate a
/// store in a function with a `try`". A block that cannot reach a handler still
/// gets the ordinary treatment, which is what says the fix is a rule about
/// exception edges rather than a switch that turns the pass off.
#[test]
fn test_a_try_that_cannot_throw_still_allows_dead_stores() {
    let out = compile_and_run(
        r#"<?php
function boom(): void { throw new RuntimeException('x'); }
function overwritten(): int {
    $t = 0;
    $t = 5;
    $t = 9;
    try { boom(); }
    catch (RuntimeException $e) { }
    return $t;
}
echo overwritten();
"#,
    );
    assert_eq!(out, "9");
}
