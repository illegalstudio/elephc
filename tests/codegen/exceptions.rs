use crate::support::*;

#[test]
fn test_exception_try_catch_same_function() {
    let out = compile_and_run(
        "<?php class MyException extends Exception {} try { throw new MyException(); } catch (MyException $e) { echo 42; }",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_builtin_exception_try_catch() {
    let out =
        compile_and_run("<?php try { throw new Exception(); } catch (Exception $e) { echo 11; }");
    assert_eq!(out, "11");
}

#[test]
fn test_builtin_exception_message_api() {
    let out = compile_and_run(
        "<?php $e = new Exception(\"boom\"); echo $e->message; echo \":\"; echo $e->getMessage();",
    );
    assert_eq!(out, "boom:boom");
}

#[test]
fn test_builtin_throwable_catches_exception() {
    let out =
        compile_and_run("<?php try { throw new Exception(); } catch (Throwable $e) { echo 12; }");
    assert_eq!(out, "12");
}

#[test]
fn test_exception_throw_during_concat_resets_concat_cursor() {
    let out = compile_and_run(
        "<?php function boom() { throw new Exception(); } try { echo \"left-\" . boom(); } catch (Exception $e) { echo json_encode([\"ok\"]); }",
    );
    assert_eq!(out, "[\"ok\"]");
}

#[test]
fn test_error_control_restores_runtime_warnings_after_exception() {
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

#[test]
fn test_exception_multi_catch_matches_each_type() {
    let out = compile_and_run(
        "<?php class AException extends Exception {} class BException extends Exception {} function boom($flag) { if ($flag) { throw new AException(); } throw new BException(); } try { boom(true); } catch (AException | BException $e) { echo 1; } try { boom(false); } catch (AException | BException $e) { echo 2; }",
    );
    assert_eq!(out, "12");
}

#[test]
fn test_exception_catch_without_variable() {
    let out =
        compile_and_run("<?php try { throw new Exception(); } catch (Exception) { echo 21; }");
    assert_eq!(out, "21");
}

#[test]
fn test_exception_catch_can_read_builtin_message() {
    let out = compile_and_run(
        "<?php try { throw new Exception(\"caught\"); } catch (Exception $e) { echo $e->getMessage(); }",
    );
    assert_eq!(out, "caught");
}

#[test]
fn test_throw_expression_in_null_coalesce() {
    let out = compile_and_run(
        "<?php $value = 42; echo $value ?? throw new Exception(); try { $missing = null; echo $missing ?? throw new Exception(); } catch (Exception) { echo 22; }",
    );
    assert_eq!(out, "4222");
}

#[test]
fn test_throw_expression_in_ternary() {
    let out = compile_and_run(
        "<?php try { echo false ? 1 : throw new Exception(); } catch (Exception) { echo 23; }",
    );
    assert_eq!(out, "23");
}

#[test]
fn test_exception_try_catch_cross_function() {
    let out = compile_and_run(
        "<?php class MyException extends Exception {} function boom() { throw new MyException(); } try { boom(); } catch (MyException $e) { echo 7; }",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_exception_nested_try_catch() {
    let out = compile_and_run(
        "<?php class InnerException extends Exception {} try { try { throw new InnerException(); } catch (InnerException $e) { echo 31; } } catch (Exception $e) { echo 99; }",
    );
    assert_eq!(out, "31");
}

#[test]
fn test_exception_throw_in_catch_rethrows() {
    let out = compile_and_run(
        "<?php class FirstException extends Exception {} class SecondException extends Exception {} try { try { throw new FirstException(); } catch (FirstException $e) { echo 32; throw new SecondException(); } } catch (SecondException $e) { echo 33; }",
    );
    assert_eq!(out, "3233");
}

#[test]
fn test_exception_throw_in_finally_overrides_prior_exception() {
    let out = compile_and_run(
        "<?php class FirstException extends Exception {} class FinalException extends Exception {} try { try { throw new FirstException(); } finally { throw new FinalException(); } } catch (FinalException $e) { echo 34; }",
    );
    assert_eq!(out, "34");
}

#[test]
fn test_exception_uncaught_reports_fatal_error() {
    let err = compile_and_run_expect_failure("<?php throw new Exception();");
    assert!(err.contains("Fatal error: uncaught exception"), "{err}");
}

#[test]
fn test_exception_with_properties() {
    let out = compile_and_run(
        "<?php class HttpException extends Exception { public $status; public function __construct() { $this->status = 404; } } try { throw new HttpException(); } catch (HttpException $e) { echo $e->status; }",
    );
    assert_eq!(out, "404");
}

#[test]
fn test_exception_try_catch_inside_loop() {
    let out = compile_and_run(
        "<?php class LoopException extends Exception {} for ($i = 0; $i < 3; $i++) { try { if ($i == 1) { throw new LoopException(); } echo $i; } catch (LoopException $e) { echo 9; } }",
    );
    assert_eq!(out, "092");
}

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

#[test]
fn test_exception_finally_runs_on_return_break_continue() {
    let out = compile_and_run(
        "<?php function f() { try { return 5; } finally { echo 1; } } echo f(); for ($i = 0; $i < 1; $i++) { try { echo 2; break; } finally { echo 3; } } for ($j = 0; $j < 2; $j++) { try { echo $j; continue; } finally { echo 9; } }",
    );
    assert_eq!(out, "15230919");
}
