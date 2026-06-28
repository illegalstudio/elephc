//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of scalar strings and null handling, including single quoted string, single quoted no escape, and single quoted escaped quote.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Single-quoted strings ---

/// Compiles `<?php echo 'hello';` and asserts stdout is `hello`.
#[test]
fn test_single_quoted_string() {
    let out = compile_and_run("<?php echo 'hello';");
    assert_eq!(out, "hello");
}

/// Compiles raw string `<?php echo 'no\n escape';` and asserts stdout is `no\n escape`
/// (backslash is literal in single-quoted strings; only `\'` is an escape).
#[test]
fn test_single_quoted_no_escape() {
    let out = compile_and_run(r"<?php echo 'no\n escape';");
    assert_eq!(out, "no\\n escape");
}

/// Compiles `<?php echo 'it\'s';` and asserts stdout is `it's` (`\'` produces a literal single quote).
#[test]
fn test_single_quoted_escaped_quote() {
    let out = compile_and_run("<?php echo 'it\\'s';");
    assert_eq!(out, "it's");
}

// --- null ---

/// Compiles `<?php echo null;` and asserts stdout is empty (null produces no output).
#[test]
fn test_null_echo_nothing() {
    let out = compile_and_run("<?php echo null;");
    assert_eq!(out, "");
}

/// Compiles `<?php $x = null; echo $x;` and asserts stdout is empty (null variable produces no output).
#[test]
fn test_null_variable_echo_nothing() {
    let out = compile_and_run("<?php $x = null; echo $x;");
    assert_eq!(out, "");
}

/// Compiles `<?php $x = null; echo is_null($x);` and asserts stdout is `1` (null is null).
#[test]
fn test_is_null_true() {
    let out = compile_and_run("<?php $x = null; echo is_null($x);");
    assert_eq!(out, "1");
}

/// Compiles `<?php $x = 42; echo is_null($x);` and asserts stdout is empty (integer is not null).
#[test]
fn test_is_null_false() {
    let out = compile_and_run("<?php $x = 42; echo is_null($x);");
    assert_eq!(out, "");
}

/// Compiles `<?php $x = null; echo $x + 5;` and asserts stdout is `5` (null coerces to 0 in arithmetic).
#[test]
fn test_null_plus_int() {
    let out = compile_and_run("<?php $x = null; echo $x + 5;");
    assert_eq!(out, "5");
}

/// Compiles `<?php $x = null; echo $x . "hello";` and asserts stdout is `hello` (null is empty string in concat).
#[test]
fn test_null_concat() {
    let out = compile_and_run("<?php $x = null; echo $x . \"hello\";");
    assert_eq!(out, "hello");
}

/// Compiles `<?php $x = null; echo $x == 0;` and asserts stdout is `1` (null equals 0 in loose comparison).
#[test]
fn test_null_equals_zero() {
    let out = compile_and_run("<?php $x = null; echo $x == 0;");
    assert_eq!(out, "1");
}

/// Compiles `<?php $y = null; $y += 10; echo $y;` and asserts stdout is `10` (null becomes 0, then adds 10).
#[test]
fn test_null_plus_assign() {
    let out = compile_and_run("<?php $y = null; $y += 10; echo $y;");
    assert_eq!(out, "10");
}

/// Compiles `<?php $x = null; $x = 42; echo $x;` and asserts stdout is `42` (null reassigned to int).
#[test]
fn test_null_reassign() {
    let out = compile_and_run("<?php $x = null; $x = 42; echo $x;");
    assert_eq!(out, "42");
}

// --- Built-in functions ---

/// Compiles `<?php echo strlen("hello");` and asserts stdout is `5`.
#[test]
fn test_strlen() {
    let out = compile_and_run("<?php echo strlen(\"hello\");");
    assert_eq!(out, "5");
}

/// Compiles `<?php echo strlen("");` and asserts stdout is `0` (empty string has length 0).
#[test]
fn test_strlen_empty() {
    let out = compile_and_run("<?php echo strlen(\"\");");
    assert_eq!(out, "0");
}

/// Compiles `<?php echo intval("42");` and asserts stdout is `42` (string to int conversion).
#[test]
fn test_intval_string() {
    let out = compile_and_run("<?php echo intval(\"42\");");
    assert_eq!(out, "42");
}

/// Compiles `<?php echo intval("-7");` and asserts stdout is `-7` (negative string to int).
#[test]
fn test_intval_negative() {
    let out = compile_and_run("<?php echo intval(\"-7\");");
    assert_eq!(out, "-7");
}

/// Compiles `<?php echo intval(42);` and asserts stdout is `42` (int passthrough, no conversion).
#[test]
fn test_intval_int_passthrough() {
    let out = compile_and_run("<?php echo intval(42);");
    assert_eq!(out, "42");
}

/// Verifies that `intval()` of a large integer string above 2^53 is exact, not routed through
/// `f64` (which would lose precision — e.g. `1234567890123456789` would become `...456768`).
/// These strings are not constant-folded (intval-of-string is evaluated at runtime), so this
/// exercises the `__rt_str_to_int` helper.
#[test]
fn test_intval_large_exact_integer_strings() {
    let out = compile_and_run(
        "<?php echo intval(\"9223372036854775805\"), \"|\", intval(\"1234567890123456789\"), \"|\", intval(\"9223372036854775000\"), \"|\", intval(\"-9223372036854775808\");",
    );
    assert_eq!(
        out,
        "9223372036854775805|1234567890123456789|9223372036854775000|-9223372036854775808"
    );
}

/// Verifies that `intval()` of out-of-range integer strings clamps to PHP_INT_MAX/PHP_INT_MIN
/// (matching PHP's `strtol`-style saturation), exercising the runtime `__rt_str_to_int` helper.
#[test]
fn test_intval_overflow_strings_clamp() {
    let out = compile_and_run(
        "<?php echo intval(\"99999999999999999999\"), \"|\", intval(\"-99999999999999999999\");",
    );
    assert_eq!(out, "9223372036854775807|-9223372036854775808");
}

/// Verifies that the `(int)` cast of a runtime (non-constant) large integer string is exact,
/// sharing the `__rt_str_to_int` helper with `intval()` (the constant `(int)"..."` form folds
/// separately at compile time, so this passes the string through a function to force runtime).
#[test]
fn test_int_cast_large_exact_integer_string_runtime() {
    let out = compile_and_run(
        "<?php function s($x): string { return $x; } echo (int) s(\"1234567890123456789\");",
    );
    assert_eq!(out, "1234567890123456789");
}

/// Verifies that `intval()` of float-form and partial numeric strings still matches PHP:
/// `"1e3"` parses as the float 1000, `"3.14"` truncates to 3, `"12abc"` stops at the first
/// non-digit, leading whitespace/sign are handled, and non-numeric strings yield 0.
#[test]
fn test_intval_float_form_and_partial_strings() {
    let out = compile_and_run(
        "<?php echo intval(\"1e3\"), \"|\", intval(\"3.14\"), \"|\", intval(\"12abc\"), \"|\", intval(\"  -5\"), \"|\", intval(\"abc\");",
    );
    assert_eq!(out, "1000|3|12|-5|0");
}

/// Regression: `intval()` of a runtime float must truncate toward zero (like the `(int)` cast), not
/// reinterpret the raw IEEE-754 bits. A non-constant float (`$x`, and a division result) exercises
/// the runtime codegen path rather than constant folding.
#[test]
fn test_intval_float_truncates() {
    let out = compile_and_run(
        "<?php $x = 830000.0; $y = -8.9; echo intval($x), '|', intval($y), '|', intval(9.0 / 2.0), '|', intval(0.0);",
    );
    assert_eq!(out, "830000|-8|4|0");
}

/// Compiles `<?php echo "before"; exit(0); echo "after";` and asserts stdout is `before`.
/// Verifies `exit` stops execution and prevents output of subsequent statements.
#[test]
fn test_exit_code() {
    // We can't easily test exit code in compile_and_run, so test that
    // exit stops execution (nothing after exit is printed)
    let out = compile_and_run("<?php echo \"before\"; exit(0); echo \"after\";");
    assert_eq!(out, "before");
}

// --- $argc ---

/// Compiles `<?php echo $argc;` and asserts stdout is `1` (test binary is run with no extra args).
#[test]
fn test_argc_exists() {
    let out = compile_and_run("<?php echo $argc;");
    // When run as a test, argc is 1 (just the binary name)
    assert_eq!(out, "1");
}

/// Compiles `<?php echo count($argv);` and asserts stdout is `1` (`$argv` has one element: the binary name).
#[test]
fn test_argv_count_exists() {
    let out = compile_and_run("<?php echo count($argv);");
    assert_eq!(out, "1");
}

/// Compiles `<?php echo $argv[0];` and asserts it ends with `/test` (script path is set by test runner).
#[test]
fn test_argv_first_entry_exists() {
    let out = compile_and_run("<?php echo $argv[0];");
    assert!(out.ends_with("/test"), "unexpected argv[0]: {out}");
}

