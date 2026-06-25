//! Purpose:
//! Integration tests for the EIR constant folding pass (`const-fold`) driven by
//! the fixed-point pass driver. Fixtures use the runtime `$argc` value so the
//! `$argc * 0` subexpression survives AST-level folding and only collapses to a
//! constant at EIR (via identity arithmetic), where this pass then folds the
//! constant-operand operation built on top of it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `$argc` is 1 when the compiled binary runs with no CLI arguments, so every
//!   `$argc * 0` evaluates to `0` and each fixture's expected output is the exact
//!   value the unfolded program would also compute — folding must be behavior
//!   preserving.
//! - The slot fixture additionally exercises propagation through local slots: a
//!   constant assigned to a variable is forwarded onto its later use by the
//!   peephole scalar load/store value-numbering, and this pass folds the
//!   resulting constant-operand multiply.

use super::*;

/// A multiply of two constants that are neither identity nor zero
/// (`5 * 5`) folds at EIR; identity arithmetic cannot reduce it.
#[test]
fn test_constant_multiply_folds() {
    let out = compile_and_run("<?php echo ($argc * 0 + 5) * ($argc * 0 + 5);");
    assert_eq!(out, "25");
}

/// A constant bitwise-and (`12 & 10`) folds at EIR to `8`.
#[test]
fn test_constant_bitwise_and_folds() {
    let out = compile_and_run("<?php echo ($argc * 0 + 12) & ($argc * 0 + 10);");
    assert_eq!(out, "8");
}

/// A constant left shift (`3 << 4`) folds at EIR to `48`.
#[test]
fn test_constant_shift_folds() {
    let out = compile_and_run("<?php echo ($argc * 0 + 3) << 4;");
    assert_eq!(out, "48");
}

/// A constant signed comparison (`0 < 5`) folds to a boolean, which then drives
/// branch simplification of the conditional to its true arm.
#[test]
fn test_constant_comparison_folds_branch() {
    let out = compile_and_run("<?php echo (($argc * 0) < 5) ? \"yes\" : \"no\";");
    assert_eq!(out, "yes");
}

/// Propagation through local slots: a constant assigned to a variable is
/// forwarded onto its later use and the resulting `7 * 3` multiply folds to `21`.
#[test]
fn test_constant_propagates_through_local_slots() {
    let out = compile_and_run(
        "<?php $a = $argc * 0 + 7; $b = $argc * 0 + 3; echo $a * $b;",
    );
    assert_eq!(out, "21");
}

/// Float constant arithmetic folds at EIR and prints the PHP-formatted result.
#[test]
fn test_constant_float_multiply_folds() {
    let out = compile_and_run("<?php $x = 2.5; $y = 2.0; echo $x * $y;");
    assert_eq!(out, "5");
}
