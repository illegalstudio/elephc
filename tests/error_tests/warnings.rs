//! Purpose:
//! Integration or regression tests for diagnostic coverage of warnings, including warning unused variable, warning byref params not flagged as unused, and warning unreachable code.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_warning_unused_variable() {
    expect_warning("<?php function foo($x) { $y = 1; return 2; }", "Unused variable: $x");
    expect_warning("<?php function foo($x) { $y = 1; return 2; }", "Unused variable: $y");
}

#[test]
fn test_warning_byref_params_not_flagged_as_unused() {
    expect_no_warning(
        "<?php function fill(int &$out): void { $out = 42; }",
        "Unused variable: $out",
    );
    expect_no_warning(
        "<?php function getColor(int $index, int &$r, int &$g, int &$b): void { $r = 255; $g = 128; $b = 0; }",
        "Unused variable: $r",
    );
    expect_no_warning(
        "<?php function getColor(int $index, int &$r, int &$g, int &$b): void { $r = 255; $g = 128; $b = 0; }",
        "Unused variable: $g",
    );
    expect_no_warning(
        "<?php function getColor(int $index, int &$r, int &$g, int &$b): void { $r = 255; $g = 128; $b = 0; }",
        "Unused variable: $b",
    );
}

#[test]
fn test_warning_unreachable_code() {
    expect_warning("<?php function foo() { return 1; echo 2; }", "Unreachable code");
}

#[test]
fn test_warning_unreachable_after_exhaustive_switch() {
    expect_warning(
        "<?php function foo($flag) { switch ($flag) { case 1: return 1; default: return 2; } echo 3; }",
        "Unreachable code",
    );
}

#[test]
fn test_warning_unreachable_after_exhaustive_try_catch() {
    expect_warning(
        "<?php function foo() { try { return 1; } catch (Exception $e) { return 2; } echo 3; }",
        "Unreachable code",
    );
}

#[test]
fn test_warning_unreachable_after_try_finally_return() {
    expect_warning(
        "<?php function foo() { try { return 1; } finally { return 2; } echo 3; }",
        "Unreachable code",
    );
}

#[test]
fn test_warning_no_unreachable_after_fallthrough_try() {
    expect_no_warning(
        "<?php function foo() { try { echo 1; } catch (Exception $e) { return 2; } echo 3; }",
        "Unreachable code",
    );
}

#[test]
fn test_warning_closure_call_marks_callable_variable_as_used() {
    expect_no_warning(
        "<?php function foo() { $f = function() { return 1; }; $f(); }",
        "Unused variable: $f",
    );
}

#[test]
fn test_warning_nested_function_is_analyzed() {
    expect_warning(
        "<?php function outer() { function inner($x) { return 1; } }",
        "Unused variable: $x",
    );
}

#[test]
fn test_warning_arrow_function_marks_outer_variable_as_used() {
    expect_no_warning(
        "<?php function outer() { $x = 1; $f = fn() => $x; }",
        "Unused variable: $x",
    );
}

#[test]
fn test_warning_unused_param_has_real_span() {
    let result = check_source_full("<?php function foo($x) { return 1; }").unwrap();
    let warning = result
        .warnings
        .iter()
        .find(|warning| warning.message.contains("Unused variable: $x"))
        .expect("expected unused param warning");
    assert!(warning.span.line > 0, "expected non-dummy span, got {:?}", warning.span);
}

#[test]
fn test_warning_final_private_method() {
    expect_warning(
        "<?php class Box { final private function seal() { return 1; } }",
        "Private methods cannot be final as they are never overridden by other classes",
    );
}

#[test]
fn test_warning_final_private_constructor_is_allowed() {
    expect_no_warning(
        "<?php class Box { final private function __construct() {} }",
        "Private methods cannot be final",
    );
}

// --- #[\Deprecated] warnings (PHP 8.4) ---

#[test]
fn test_warning_deprecated_function_call() {
    expect_warning(
        "<?php #[\\Deprecated] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

#[test]
fn test_warning_deprecated_function_includes_reason() {
    expect_warning(
        "<?php #[\\Deprecated(\"use newApi()\")] function oldApi(): int { return 1; } echo oldApi();",
        "Call to deprecated function: oldApi() — use newApi()",
    );
}

#[test]
fn test_warning_deprecated_method_call() {
    expect_warning(
        "<?php class Svc { #[\\Deprecated] public function fetch(): int { return 1; } } $s = new Svc(); echo $s->fetch();",
        "Call to deprecated method: Svc::fetch()",
    );
}

#[test]
fn test_warning_deprecated_static_method_call() {
    expect_warning(
        "<?php class Reg { #[\\Deprecated(\"removed in v3\")] public static function lookup(): int { return 1; } } echo Reg::lookup();",
        "Call to deprecated static method: Reg::lookup() — removed in v3",
    );
}

#[test]
fn test_warning_deprecated_unqualified_form_is_recognized() {
    expect_warning(
        "<?php #[Deprecated] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

#[test]
fn test_warning_deprecated_import_alias_is_recognized() {
    expect_warning(
        "<?php use Deprecated as Old; #[Old] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

#[test]
fn test_warning_no_deprecation_for_qualified_lookalike() {
    expect_no_warning(
        "<?php #[Foo\\Deprecated] function legacy(): int { return 1; } echo legacy();",
        "deprecated function",
    );
}

#[test]
fn test_warning_no_deprecation_for_namespaced_unqualified_lookalike() {
    expect_no_warning(
        "<?php namespace N; #[Deprecated] function legacy(): int { return 1; } echo legacy();",
        "deprecated function",
    );
}

#[test]
fn test_warning_no_deprecation_when_not_called() {
    expect_no_warning(
        "<?php #[\\Deprecated] function legacy(): int { return 1; } echo 1;",
        "deprecated function",
    );
}

#[test]
fn test_warning_no_deprecation_for_undeprecated_function() {
    expect_no_warning(
        "<?php function fresh(): int { return 1; } echo fresh();",
        "deprecated",
    );
}
