//! Purpose:
//! Integration tests for the EIR identity arithmetic folding pass driven by the
//! fixed-point pass driver. Fixtures use the runtime `$argc` value so an
//! identity op with a literal operand survives AST-level folding and reaches
//! EIR, where the identity pass must rewrite it while preserving behavior.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `$argc` is 1 when the compiled binary runs with no CLI arguments.
//! - The side-effect fixture proves folding `expr * 1` to `expr` keeps the
//!   operand's defining (side-effecting) call instruction.

use super::*;

/// `$argc * 1` folds to `$argc` at the IR level and still prints the value.
#[test]
fn test_identity_mul_one_preserves_value() {
    let out = compile_and_run("<?php echo $argc * 1;");
    assert_eq!(out, "1");
}

/// `$argc + 0` folds to `$argc` and still prints the value.
#[test]
fn test_identity_add_zero_preserves_value() {
    let out = compile_and_run("<?php echo $argc + 0;");
    assert_eq!(out, "1");
}

/// `$argc << 0` folds to `$argc` and still prints the value.
#[test]
fn test_identity_shift_zero_preserves_value() {
    let out = compile_and_run("<?php echo $argc << 0;");
    assert_eq!(out, "1");
}

/// `$argc * 0` folds to the integer constant `0`.
#[test]
fn test_identity_mul_zero_yields_zero() {
    let out = compile_and_run("<?php echo $argc * 0;");
    assert_eq!(out, "0");
}

/// Folding `bump() * 1` to `bump()` must keep the side-effecting call: the echo
/// inside `bump()` still runs and its return value flows through.
#[test]
fn test_identity_fold_preserves_operand_side_effects() {
    let out = compile_and_run(
        "<?php function bump() { echo \"x\"; return 3; } echo bump() * 1;",
    );
    assert_eq!(out, "x3");
}

/// Regression: `$x / 1` must NOT be folded to its I64 operand — PHP's `/` on two ints yields
/// an int only when the division is exact, so the result is a boxed int-or-float and folding
/// reinterprets its bits. `var_dump($argc / 1)` and `var_dump(($argc + 2) / 2)` print what
/// php prints: `int(1)` and `float(1.5)`.
#[test]
fn test_identity_div_one_keeps_php_division_type() {
    let out = compile_and_run("<?php var_dump($argc / 1); var_dump(($argc + 2) / 2);");
    assert_eq!(out, "int(1)\nfloat(1.5)\n");
}

/// Regression: a `bool` shares `i64` storage with an `int`, and folding `$b * 1`, `1 * $b` or
/// `$b + 0` to the operand made the result keep the operand's PHP type: `var_dump` printed
/// `bool(true)` where php prints `int(1)`. The fold now also requires the PHP types to agree.
/// `--no-ir-opt` always printed php's answer.
#[test]
fn test_identity_fold_keeps_arithmetic_result_type_for_bool_operand() {
    let out = compile_and_run("<?php $b = $argc > 0; var_dump($b * 1, 1 * $b, $b + 0, $b * 2);");
    assert_eq!(out, "int(1)\nint(1)\nint(1)\nint(2)\n");
}

/// Regression (the dangerous manifestation): a folded `$x / 1` feeding a float
/// operation would read garbage bits. With the fold correctly suppressed, the float
/// arithmetic is exact.
#[test]
fn test_identity_div_one_into_float_op_is_exact() {
    let out = compile_and_run("<?php echo ($argc / 1) + 0.5;");
    assert_eq!(out, "1.5");
}
