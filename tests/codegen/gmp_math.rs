//! Purpose:
//! End-to-end tests for GMP operands in PHP's math builtins and scalar casts.
//!
//! Called from:
//! - `cargo test --test codegen_tests gmp_math` through the codegen test module.
//!
//! Key details:
//! - Expected output is php-src 8.5 output with ext/gmp; only `gmp_strval()` and scalar
//!   results are printed, never object handles.
//! - `min()`/`max()` encode php-src's asymmetric tie rule: the two-argument frameless
//!   `min()` keeps its right operand, while `max()` and every array form keep the first.

use crate::support::*;

/// Verifies n-ary, array, and mixed-stored GMP operands plus php-src's tie rules.
#[test]
fn test_gmp_min_max_nary_array_and_ties() {
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$a = gmp_init(1); $b = gmp_init(1); $c = gmp_init(1);
echo min($a, $b) === $b ? "b" : "?", "\n";
echo max($a, $b) === $a ? "a" : "?", "\n";
echo min($a, $b, $c) === $a ? "a" : "?", "\n";
echo min([$a, $b, $c]) === $a ? "a" : "?", "\n";
echo gmp_strval(max(gmp_init(1), gmp_init(5), gmp_init(3))), "\n";
echo gmp_strval(min([gmp_init(4), gmp_init(2)])), "\n";
$m = ['x' => gmp_init(7)];
echo gmp_strval(min($m['x'], 3)), "|", gmp_strval(max($m['x'], 3)), "\n";
echo gmp_strval(min(3, $m['x'])), "|", gmp_strval(max(3, $m['x'])), "\n";
$values = [3, 1, 2];
echo min(array_values($values)), "|", max(array_values($values)), "\n";
echo min([gmp_init(5), 2]), "|", gmp_strval(max([gmp_init(5), 2])), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "b\na\na\na\n5\n2\n3|7\n3|7\n1|3\n2|5\n");
}

/// Verifies mixed-stored GMP reaches `pow()`, `intval()`, `floatval()` and explicit casts.
#[test]
fn test_gmp_pow_intval_floatval_and_casts() {
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$g = gmp_init(21);
$a = ['x' => $g, 's' => 'ff'];
echo gmp_strval(pow($a['x'], 3)), "\n";
echo intval($g), "|", intval($g, 16), "|", floatval($g), "\n";
echo intval($a['x']), "|", (int) $a['x'], "|", floatval($a['x']), "|", (float) $a['x'], "\n";
echo intval($a['s'], 16), "|", intval($a['s']), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "9261\n21|21|21\n21|21|21|21\n255|0\n");
}

/// Verifies a compiler-generated GMP prelude deprecation reports the caller's source file.
///
/// `__elephc_gmp_float_to_int` raises the deprecation from injected prelude code, so the
/// diagnostic writer reads the caller location `prepare_call_site` published. The writer
/// takes that file in x1/x2 on AArch64 and rdi/rsi on x86_64; using the string-result
/// registers instead printed an empty filename on x86_64.
#[test]
fn test_gmp_float_operator_deprecation_reports_source_file() {
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$num = gmp_init(42);
var_dump($num + 42.5);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    let first = out.stdout.lines().next().unwrap_or_default();
    assert!(
        first.starts_with(
            "Deprecated: Implicit conversion from float 42.5 to int loses precision in "
        ),
        "unexpected diagnostic: {first}"
    );
    assert!(
        !first.contains("in  on line"),
        "diagnostic omitted the source file: {first}"
    );
    assert!(
        first.contains("test.php on line 3"),
        "diagnostic did not name the caller file and line: {first}"
    );
    assert!(out.stdout.contains("string(2) \"84\""), "stdout: {}", out.stdout);
}

/// Verifies a caught GMP exception reports the user call line, not a generated prelude line.
#[test]
fn test_gmp_exception_reports_the_call_site_line() {
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
try { gmp_init('zz'); } catch (Throwable $e) { echo $e->getLine(), "\n"; }
try { gmp_strval(gmp_init(1), 99); } catch (Throwable $e) { echo $e->getLine(), "\n"; }
try { gmp_div_q(gmp_init(1), gmp_init(0)); } catch (Throwable $e) { echo $e->getLine(), "\n"; }
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "2\n3\n4\n");
}

/// Verifies `abs()`/`round()` reject a GMP object while `is_numeric()` answers false.
#[test]
fn test_gmp_rejected_by_scalar_only_numeric_builtins() {
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$g = gmp_init(7);
var_dump(is_numeric($g));
try { var_dump(abs($g)); } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { round($g); } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\n\
TypeError: abs(): Argument #1 ($num) must be of type int|float, GMP given\n\
TypeError: round(): Argument #1 ($num) must be of type int|float, GMP given\n"
    );
}

/// Verifies a GMP capture keeps operator dispatch inside an arrow-function body.
#[test]
fn test_gmp_closure_capture_operators() {
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$g = gmp_init(5);
$mod = fn() => $g % 3;
$div = fn() => $g / 2;
$sum = fn() => $g + 2;
echo gmp_strval($mod()), "|", gmp_strval($div()), "|", gmp_strval($sum()), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "2|2|7\n");
}
