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

/// Regression: PHP's `/` is float-producing even on two int operands (it lowers to a
/// float-typed `idiv`), so `$x / 1` must NOT be folded to its I64 operand — doing so
/// reinterprets the integer bits when the result is consumed as a float. `var_dump`
/// must report a float, exactly as the unoptimized path does.
#[test]
fn test_identity_div_one_keeps_float_type() {
    let out = compile_and_run("<?php var_dump($argc / 1);");
    assert_eq!(out, "float(1)\n");
}

/// Regression (the dangerous manifestation): a folded `$x / 1` feeding a float
/// operation would read garbage bits. With the fold correctly suppressed, the float
/// arithmetic is exact.
#[test]
fn test_identity_div_one_into_float_op_is_exact() {
    let out = compile_and_run("<?php echo ($argc / 1) + 0.5;");
    assert_eq!(out, "1.5");
}
