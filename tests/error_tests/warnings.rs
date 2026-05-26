//! Purpose:
//! Integration or regression tests for diagnostic coverage of warnings, including warning unused variable, warning byref params not flagged as unused, and warning unreachable code.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

// Verifies that the compiler emits "Unused variable" warnings for function parameters
// and local assignments that are never read.
#[test]
fn test_warning_unused_variable() {
    expect_warning("<?php function foo($x) { $y = 1; return 2; }", "Unused variable: $x");
    expect_warning("<?php function foo($x) { $y = 1; return 2; }", "Unused variable: $y");
}

// Verifies that by-reference parameters are NOT flagged as unused even when they
// are not explicitly read in the function body, because they are semantically
// output parameters — the callee writes to the caller's variable.
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

// Verifies that code immediately following a `return` statement within a function
// body is flagged as unreachable.
#[test]
fn test_warning_unreachable_code() {
    expect_warning("<?php function foo() { return 1; echo 2; }", "Unreachable code");
}

// Verifies that code after a `switch` that covers all cases (including `default`)
// is flagged as unreachable.
#[test]
fn test_warning_unreachable_after_exhaustive_switch() {
    expect_warning(
        "<?php function foo($flag) { switch ($flag) { case 1: return 1; default: return 2; } echo 3; }",
        "Unreachable code",
    );
}

// Verifies that code after a `try-catch` where both branches return is flagged
// as unreachable.
#[test]
fn test_warning_unreachable_after_exhaustive_try_catch() {
    expect_warning(
        "<?php function foo() { try { return 1; } catch (Exception $e) { return 2; } echo 3; }",
        "Unreachable code",
    );
}

// Verifies that code after a `try-finally` where the `finally` block performs a
// return is flagged as unreachable (finally's return takes precedence over the
// try's return from the caller's perspective).
#[test]
fn test_warning_unreachable_after_try_finally_return() {
    expect_warning(
        "<?php function foo() { try { return 1; } finally { return 2; } echo 3; }",
        "Unreachable code",
    );
}

// Verifies that code after a `try-catch` where only the `catch` branch returns
// is NOT flagged as unreachable, because the `try` block may complete without
// returning and fall through to the subsequent code.
#[test]
fn test_warning_no_unreachable_after_fallthrough_try() {
    expect_no_warning(
        "<?php function foo() { try { echo 1; } catch (Exception $e) { return 2; } echo 3; }",
        "Unreachable code",
    );
}

// Verifies that a closure variable is not flagged as unused when the closure is
// invoked (the variable holds a callable that is called, so it is used).
#[test]
fn test_warning_closure_call_marks_callable_variable_as_used() {
    expect_no_warning(
        "<?php function foo() { $f = function() { return 1; }; $f(); }",
        "Unused variable: $f",
    );
}

// Verifies that unused parameters of nested function declarations are analyzed
// and produce warnings, not silently ignored.
#[test]
fn test_warning_nested_function_is_analyzed() {
    expect_warning(
        "<?php function outer() { function inner($x) { return 1; } }",
        "Unused variable: $x",
    );
}

// Verifies that an outer variable captured by an arrow function is not flagged
// as unused, because the arrow function implicitly returns the variable.
#[test]
fn test_warning_arrow_function_marks_outer_variable_as_used() {
    expect_no_warning(
        "<?php function outer() { $x = 1; $f = fn() => $x; }",
        "Unused variable: $x",
    );
}

// Verifies that the span of an unused parameter warning is a real source span
// (line > 0) rather than a dummy fallback span, ensuring the diagnostic points
// to the correct source location.
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

// Verifies that marking a private method as final produces a warning, because
// private methods cannot be overridden by child classes, making the `final`
// modifier redundant.
#[test]
fn test_warning_final_private_method() {
    expect_warning(
        "<?php class Box { final private function seal() { return 1; } }",
        "Private methods cannot be final as they are never overridden by other classes",
    );
}

// Verifies that a final private constructor does NOT produce a warning. Unlike
// regular private methods, a final private constructor is a legitimate pattern
// (e.g., singleton enforcement) and is not redundant.
#[test]
fn test_warning_final_private_constructor_is_allowed() {
    expect_no_warning(
        "<?php class Box { final private function __construct() {} }",
        "Private methods cannot be final",
    );
}

// --- #[\Deprecated] warnings (PHP 8.4) ---

// Verifies that calling a function marked with #[Deprecated] without arguments
// emits a deprecation warning.
#[test]
fn test_warning_deprecated_function_call() {
    expect_warning(
        "<?php #[\\Deprecated] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

// Verifies that a deprecation reason provided via #[Deprecated("use newApi()")] is
// included in the emitted warning message after a dash separator.
#[test]
fn test_warning_deprecated_function_includes_reason() {
    expect_warning(
        "<?php #[\\Deprecated(\"use newApi()\")] function oldApi(): int { return 1; } echo oldApi();",
        "Call to deprecated function: oldApi() — use newApi()",
    );
}

// Verifies that calling an instance method marked with #[Deprecated] emits a
// deprecation warning mentioning the method name.
#[test]
fn test_warning_deprecated_method_call() {
    expect_warning(
        "<?php class Svc { #[\\Deprecated] public function fetch(): int { return 1; } } $s = new Svc(); echo $s->fetch();",
        "Call to deprecated method: Svc::fetch()",
    );
}

// Verifies that calling a static method marked with #[Deprecated] emits a
// deprecation warning including the reason string when provided.
#[test]
fn test_warning_deprecated_static_method_call() {
    expect_warning(
        "<?php class Reg { #[\\Deprecated(\"removed in v3\")] public static function lookup(): int { return 1; } } echo Reg::lookup();",
        "Call to deprecated static method: Reg::lookup() — removed in v3",
    );
}

// Verifies that the short `#[Deprecated]` attribute (without backslash) on a
// function is recognized as a deprecation marker and produces a warning when
// the function is called.
#[test]
fn test_warning_deprecated_unqualified_form_is_recognized() {
    expect_warning(
        "<?php #[Deprecated] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

// Verifies that a `use Deprecated as Old;` import alias is accepted as a valid
// deprecation attribute and produces a warning when the aliased function is called.
#[test]
fn test_warning_deprecated_import_alias_is_recognized() {
    expect_warning(
        "<?php use Deprecated as Old; #[Old] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

// Verifies that `#[Foo\Deprecated]` (a namespaced attribute that is not the
// built-in `Deprecated`) does NOT produce a deprecation warning.
#[test]
fn test_warning_no_deprecation_for_qualified_lookalike() {
    expect_no_warning(
        "<?php #[Foo\\Deprecated] function legacy(): int { return 1; } echo legacy();",
        "deprecated function",
    );
}

// Verifies that `#[Deprecated]` inside a namespace does NOT produce a deprecation
// warning when called via an unqualified name, because in namespaced files the
// compiler's builtin attribute fallback is not applied to local names.
#[test]
fn test_warning_no_deprecation_for_namespaced_unqualified_lookalike() {
    expect_no_warning(
        "<?php namespace N; #[Deprecated] function legacy(): int { return 1; } echo legacy();",
        "deprecated function",
    );
}

// Verifies that a deprecated function does NOT produce a deprecation warning
// when it is declared but never called.
#[test]
fn test_warning_no_deprecation_when_not_called() {
    expect_no_warning(
        "<?php #[\\Deprecated] function legacy(): int { return 1; } echo 1;",
        "deprecated function",
    );
}

// Verifies that a regular (non-deprecated) function does NOT produce any
// deprecation warning when called.
#[test]
fn test_warning_no_deprecation_for_undeprecated_function() {
    expect_no_warning(
        "<?php function fresh(): int { return 1; } echo fresh();",
        "deprecated",
    );
}
